# Changelog

## 0.2.0 — 2026-10-28

- Add algorithm V2: lossless event-field separation, fixed-width time/coordinate
  differences, same-width coordinate ZigZag and LZMA1 compression.
- Add framing-aware preprocessing for AVF, MVF 0.97/2006/2007, RMV 1/2,
  EVF 0.3/0.4 and EVFS 0.0. Existing EVF coordinate increments and RMV 2 reduced
  moves also receive ZigZag. Unknown layouts retain a raw-byte compression path.
- Make V2 the default while retaining V1 compression and decompression. Choose
  the algorithm explicitly with Rust's `Algorithm`, Python's `algorithm_version`,
  or the JavaScript `compress` function's third argument.
- Preserve every original byte, including event aliases, MVF residual bits,
  metadata, checksums, signatures and opaque tails. The encoder compares raw and
  preprocessed candidates including descriptor overhead.
- Add independent V2 fixtures, format research and reproducible benchmarks.
  The selected 1,038-file corpus is 31.65% smaller than V1 at the same level 6;
  compression time is about 1.77 times V1. These are dataset-specific results
  from the same five-round experiment; see
  [the benchmark](docs/benchmarks/2026-10-27-v1-v2-reduced.md).

The MSRC container remains version 1. Both algorithm versions use LZMA1;
the V2 name identifies preprocessing and payload layout, not LZMA2.

## 0.1.0

- Initial Rust LZMA1 library, versioned MSRC container, Python bindings distributed
  through GitHub Releases, and public Node.js/web WASM package `@putianyi888/msrc`.
