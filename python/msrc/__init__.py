"""Lossless Minesweeper replay compression, backed by the MSRC Rust core."""

from ._native import ArchiveInfo, __version__, compress, decompress, inspect

__all__ = ["ArchiveInfo", "__version__", "compress", "decompress", "inspect"]
