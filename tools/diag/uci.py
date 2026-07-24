"""Minimal UCI driver shared by the strength-diagnostic scripts (TASK-82).

Spawns an engine as a subprocess and speaks just enough UCI to run fixed-depth
and fixed-time searches, read the per-iteration ``info`` lines, and query a
static evaluation. Deliberately dependency-free (stdlib only) so it runs under
the bare Python on any measurement host.
"""

from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass


def load_fens(path: str) -> list[tuple[str, str]]:
    """Read a bench/EPD file into ``(fen, label)`` pairs.

    Blank lines and ``#`` comments are skipped. Only the six FEN fields are
    kept; a trailing EPD operation block or a ``;`` comment supplies the label.
    """
    out: list[tuple[str, str]] = []
    with open(path, encoding="utf-8") as handle:
        for raw in handle:
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            label = ""
            if ";" in line:
                line, _, rest = line.partition(";")
                label = rest.strip()
            fields = line.split()
            if len(fields) < 6:
                continue
            fen = " ".join(fields[:6])
            out.append((fen, label))
    return out


def parse_info(line: str) -> dict[str, int]:
    """Extract the numeric fields of interest from a UCI ``info`` line."""
    fields: dict[str, int] = {}
    for key in ("depth", "nodes", "nps", "time"):
        match = re.search(rf"\b{key}\s+(\d+)", line)
        if match:
            fields[key] = int(match.group(1))
    return fields


@dataclass
class SearchResult:
    depth: int
    nodes: int
    nps: int
    time_ms: int


class Engine:
    """A UCI engine subprocess held open across many positions."""

    def __init__(self, cmd: list[str], options: dict[str, str] | None = None):
        self.proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
        )
        self._send("uci")
        self._read_until(lambda line: line.strip() == "uciok")
        for name, value in (options or {}).items():
            self._send(f"setoption name {name} value {value}")
        self.isready()

    def _send(self, cmd: str) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(cmd + "\n")
        self.proc.stdin.flush()

    def _read_until(self, done):
        assert self.proc.stdout is not None
        lines: list[str] = []
        while True:
            line = self.proc.stdout.readline()
            if line == "":
                raise RuntimeError("engine closed its output unexpectedly")
            lines.append(line)
            if done(line):
                return lines

    def isready(self) -> None:
        self._send("isready")
        self._read_until(lambda line: line.strip() == "readyok")

    def new_game(self) -> None:
        self._send("ucinewgame")
        self.isready()

    def set_position(self, fen: str) -> None:
        self._send(f"position fen {fen}")

    def _search(self, fen: str, go: str) -> SearchResult:
        self.set_position(fen)
        self._send(go)
        lines = self._read_until(lambda line: line.startswith("bestmove"))
        # The deepest completed iteration is the last full-width `info` line
        # before `bestmove`; take its node/time/nps counters.
        best: dict[str, int] = {}
        for line in lines:
            if not line.startswith("info"):
                continue
            info = parse_info(line)
            if "depth" in info and "nodes" in info:
                if not best or info["depth"] >= best.get("depth", 0):
                    best = info
        return SearchResult(
            depth=best.get("depth", 0),
            nodes=best.get("nodes", 0),
            nps=best.get("nps", 0),
            time_ms=best.get("time", 0),
        )

    def command(self, cmd: str, until_prefix: str) -> list[str]:
        """Send ``cmd`` and return all output lines up to and including the first
        line that starts with ``until_prefix``."""
        self._send(cmd)
        return self._read_until(lambda line: line.startswith(until_prefix))

    def go_depth(self, fen: str, depth: int) -> SearchResult:
        return self._search(fen, f"go depth {depth}")

    def go_movetime(self, fen: str, movetime_ms: int) -> SearchResult:
        return self._search(fen, f"go movetime {movetime_ms}")

    def quit(self) -> None:
        try:
            self._send("quit")
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()
