"""A reference encoder for the packed sample format, used only by the tests.

This is the inverse of :func:`data.decode` written independently from it, so a
round-trip test exercises the byte layout from both directions without depending
on the Rust generator being built. It follows the layout documented in
``data.py`` and in ``engine::selfplay::format`` byte for byte.
"""

from __future__ import annotations

import numpy as np

from data import HEADER_SIZE, MAGIC, RECORD_SIZE, FORMAT_VERSION

# Piece codes shared with the Rust encoding: 1=P..6=K white, 7=p..12=k black.
WHITE_PAWN, WHITE_KNIGHT, WHITE_BISHOP, WHITE_ROOK, WHITE_QUEEN, WHITE_KING = range(1, 7)
BLACK_PAWN, BLACK_KNIGHT, BLACK_BISHOP, BLACK_ROOK, BLACK_QUEEN, BLACK_KING = range(7, 13)

_NO_EP = 0xFF


def encode_record(
    pieces: dict[int, int],
    *,
    black_to_move: bool = False,
    score: int = 0,
    wdl: int = 1,
) -> np.ndarray:
    """Build one 32-byte record. ``pieces`` maps square index (A1=0) to a piece
    code. Metadata fields not relevant to decoding features are left at zero."""
    record = np.zeros(RECORD_SIZE, dtype=np.uint8)

    occupancy = 0
    for sq in pieces:
        occupancy |= 1 << sq
    record[0:8] = np.frombuffer(int(occupancy).to_bytes(8, "little"), dtype=np.uint8)

    for slot, sq in enumerate(sorted(pieces)):
        code = pieces[sq]
        byte = 8 + slot // 2
        if slot % 2 == 0:
            record[byte] |= code
        else:
            record[byte] |= code << 4

    record[24] = 1 if black_to_move else 0
    record[25] = _NO_EP
    record[29:31] = np.frombuffer(int(score).to_bytes(2, "little", signed=True), dtype=np.uint8)
    record[31] = wdl
    return record


def mirror(pieces: dict[int, int]) -> dict[int, int]:
    """Colour-flip and vertically flip a placement: the input to the "equal
    evaluation from the side to move" invariant."""
    flipped = {}
    for sq, code in pieces.items():
        colour_flipped = code + 6 if code <= 6 else code - 6
        flipped[sq ^ 56] = colour_flipped
    return flipped


def encode_stream(records) -> bytes:
    """Prefix a list of records with the 8-byte stream header."""
    header = bytearray(HEADER_SIZE)
    header[0:4] = MAGIC
    header[4:6] = int(FORMAT_VERSION).to_bytes(2, "little")
    header[6:8] = int(RECORD_SIZE).to_bytes(2, "little")
    body = b"".join(bytes(r) for r in records)
    return bytes(header) + body
