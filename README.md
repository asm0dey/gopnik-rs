# gopnik-rs

A Rust reimplementation of a 2003 Borland Pascal DOS game (`g.exe`).

## Reference Corpus

The `orig/` directory contains the original game binary and save files, sourced from the 2003 release. These files are read-only reference data used by all analysis and porting tasks. They must never be modified.

- `g.exe` — The original DOS game binary
- `PLACES.SAV`, `SAVE_R*.SAV` — Game save files for reference and testing
