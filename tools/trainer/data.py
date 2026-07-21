"""A fast dataloader for the packed self-play sample format.

The network is tiny (order 10^5 parameters), so training is dataloader-bound: a
naive per-sample Python loop that decodes one 32-byte record at a time would
starve the GPU. This module instead memory-maps the file and decodes a whole
batch of records at once with vectorised NumPy, turning each batch directly into
the sparse ``(indices, offsets)`` form ``torch.nn.EmbeddingBag`` consumes.

The on-disk format is the one the Rust generator writes (`engine::selfplay::
format`): an 8-byte stream header, then fixed 32-byte little-endian records.

    | Bytes | Field     | Meaning                                              |
    |-------|-----------|------------------------------------------------------|
    | 0..8  | occupancy | Bit i set iff square i (A1=0) holds a piece.          |
    | 8..24 | pieces    | One 4-bit code per occupied square, ascending square |
    |       |           | order, low nibble first. 1=P..6=K white, 7=p..12=k.  |
    | 24    | flags     | bit0 side to move (0 White, 1 Black); bits1..5 castle |
    | 25    | ep        | En-passant target square, or 0xFF for none.          |
    | 26    | halfmove  | Fifty-move clock.                                    |
    | 27..29| fullmove  | Full move number.                                   |
    | 29..31| score     | Search score, raw i16 (mate band preserved).         |
    | 31    | wdl       | Outcome from the side to move: 0 loss, 1 draw, 2 win. |

The feature index for a piece follows the contract exactly. For a piece of
colour c and type t on square sq, its index for perspective p is::

    oriented = sq            if p is that piece-holder's own White perspective
             = sq ^ 56       for the Black perspective (vertical flip)
    pt0      = type ordinal, Pawn=0 .. King=5
    side     = 0 if c == p (friendly) else 1 (enemy)
    index    = oriented + 64 * pt0 + 384 * side          in 0..768

Both perspectives share one accumulator's feature transformer; only the index
formula differs, and the two use the same active-square set, so a single
per-sample ``offsets`` array serves both.
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np

# Stream-header constants, matching the Rust writer. A file that disagrees is
# rejected rather than misread.
MAGIC = b"SBRG"
FORMAT_VERSION = 1
RECORD_SIZE = 32
HEADER_SIZE = 8

# Byte offsets within a record.
_OCC = slice(0, 8)
_PIECES = slice(8, 24)
_FLAGS = 24
_SCORE = slice(29, 31)
_WDL = 31

_NUM_SQUARES = 64
_NO_PIECE_SLOTS = 32  # two nibbles per byte across the 16-byte piece block


class FormatError(ValueError):
    """The stream header or a record did not match the packed format."""


@dataclass
class Batch:
    """A decoded batch, in the sparse form the model consumes. ``stm_*`` and
    ``nstm_*`` are the ``EmbeddingBag`` inputs for the side-to-move and other
    perspective; both use one shared ``offsets`` because the active squares are
    the same, only the index formula differs. ``score`` is the raw i16 search
    score and ``wdl`` is 0/1/2, both left as-is so the trainer owns the target
    formulation (which needs SCALE and lambda)."""

    stm_indices: np.ndarray
    nstm_indices: np.ndarray
    offsets: np.ndarray
    score: np.ndarray
    wdl: np.ndarray

    def __len__(self) -> int:
        return self.offsets.shape[0]


def decode(records: np.ndarray) -> Batch:
    """Decode a ``[B, 32]`` uint8 array of raw records into a :class:`Batch`.

    Everything is vectorised over the batch: no per-sample Python loop touches a
    record, which is what keeps the loader ahead of the GPU.
    """
    if records.ndim != 2 or records.shape[1] != RECORD_SIZE:
        raise FormatError(f"expected a [B, {RECORD_SIZE}] record array, got {records.shape}")
    batch = records.shape[0]

    # Occupancy: bit i (little-endian within the 8 bytes) marks square i.
    occupied = np.unpackbits(records[:, _OCC], axis=1, bitorder="little").astype(bool)
    counts = occupied.sum(axis=1, dtype=np.int64)  # pieces per position

    # Piece codes: 16 bytes -> 32 nibbles, low nibble of each byte first, in
    # ascending occupied-square order.
    nib_bytes = records[:, _PIECES]
    nibbles = np.empty((batch, _NO_PIECE_SLOTS), dtype=np.int64)
    nibbles[:, 0::2] = nib_bytes & 0x0F
    nibbles[:, 1::2] = nib_bytes >> 4

    # Scatter each square's code back to its square: the k-th occupied square
    # (k = cumulative occupancy - 1) takes the k-th nibble. Non-occupied squares
    # get a harmless value that the occupancy mask discards below.
    slot = np.cumsum(occupied, axis=1, dtype=np.int64) - 1
    np.clip(slot, 0, _NO_PIECE_SLOTS - 1, out=slot)
    code = np.take_along_axis(nibbles, slot, axis=1)  # [B, 64], valid where occupied

    if int(counts.max(initial=0)) > _NO_PIECE_SLOTS:
        raise FormatError("a record's occupancy names more than 32 pieces")

    squares = np.arange(_NUM_SQUARES, dtype=np.int64)[None, :]
    is_white_piece = code <= 6
    piece_type = (code - 1) % 6  # Pawn=0 .. King=5, for either colour

    # White perspective: squares upright, own (white) pieces are friendly.
    white_side = np.where(is_white_piece, 0, 1)
    white_idx = squares + 64 * piece_type + 384 * white_side
    # Black perspective: board vertically flipped, black pieces are friendly.
    black_side = np.where(is_white_piece, 1, 0)
    black_idx = (squares ^ 56) + 64 * piece_type + 384 * black_side

    # Side to move selects which perspective is "stm". flags bit0: 0 => White.
    stm_is_white = (records[:, _FLAGS] & 1) == 0
    stm_idx = np.where(stm_is_white[:, None], white_idx, black_idx)
    nstm_idx = np.where(stm_is_white[:, None], black_idx, white_idx)

    # Flatten to EmbeddingBag form. occupied is row-major, so masking keeps the
    # per-sample, ascending-square order the offsets assume.
    offsets = np.zeros(batch, dtype=np.int64)
    np.cumsum(counts[:-1], out=offsets[1:])
    stm_flat = stm_idx[occupied]
    nstm_flat = nstm_idx[occupied]

    # Score: reassemble the little-endian i16 without a non-contiguous view.
    raw = (records[:, 29].astype(np.uint16) | (records[:, 30].astype(np.uint16) << 8)).astype(
        np.uint16
    )
    score = raw.view(np.int16).astype(np.int64)
    wdl = records[:, _WDL].astype(np.int64)

    return Batch(
        stm_indices=stm_flat,
        nstm_indices=nstm_flat,
        offsets=offsets,
        score=score,
        wdl=wdl,
    )


class PackedData:
    """A memory-mapped packed-sample file. Records are addressed by index and
    decoded a batch at a time; nothing is copied until a batch is gathered."""

    def __init__(self, path) -> None:
        with open(path, "rb") as handle:
            header = handle.read(HEADER_SIZE)
        if len(header) < HEADER_SIZE:
            raise FormatError("file is shorter than the 8-byte stream header")
        if header[0:4] != MAGIC:
            raise FormatError("not a Seaborg sample stream (bad magic)")
        version = int.from_bytes(header[4:6], "little")
        if version != FORMAT_VERSION:
            raise FormatError(f"unsupported sample format version {version}")
        record_size = int.from_bytes(header[6:8], "little")
        if record_size != RECORD_SIZE:
            raise FormatError(f"unexpected record size {record_size}")

        # The records follow the header; a memmap avoids reading the whole file
        # into RAM, so hundreds of millions of samples stream from disk.
        flat = np.memmap(path, dtype=np.uint8, mode="r", offset=HEADER_SIZE)
        if flat.size % RECORD_SIZE != 0:
            raise FormatError("file ends partway through a record")
        self.records = flat.reshape(-1, RECORD_SIZE)

    def __len__(self) -> int:
        return self.records.shape[0]

    def batch(self, indices: np.ndarray) -> Batch:
        """Gather and decode the records at ``indices`` (fancy indexing copies
        them into a contiguous array first)."""
        return decode(np.asarray(self.records[indices]))


def iter_batches(data: PackedData, indices: np.ndarray, batch_size: int):
    """Yield decoded batches over ``indices`` in order. Shuffle by permuting
    ``indices`` before calling."""
    for start in range(0, len(indices), batch_size):
        yield data.batch(indices[start : start + batch_size])
