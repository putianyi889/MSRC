# Initial research and scope

Reviewed 2026-10-27. These projects are references, not vendored dependencies.
The first version compresses complete replay bytes, so it does not need to copy
or reimplement their parsers.

## Existing replay formats

The follow-up [mouse event storage investigation](research/mouse-events/README.md)
records version-specific byte/bit layouts, upstream discrepancies, and local
corpus measurements, including MVF samples and EVFS embedded replays.

- [ms-toollib](https://github.com/eee555/ms-toollib) supplies Rust replay tooling
  and language bindings. Its [EVF specification](https://github.com/eee555/ms-toollib/blob/main/evf%20format%20(evf%E6%A0%87%E5%87%86).md)
  describes a structured replay format with its own evolving versions.
- [minesweeper-rawvf](https://github.com/thefinerminer/minesweeper-rawvf) converts
  several native replay formats into human-readable RAWVF. Its README describes
  RAW 5/6, mouse events, timestamps and metadata. Format conversion is a useful
  analysis tool but is not proof of byte-for-byte recovery of a native file.
- [rmv_spec](https://github.com/ralokt/rmv_spec) documents RMV, including an
  incompatible v2, extension properties and compact relative mouse events. The
  repository labels the specification as a draft.

These references justify treating the native file as opaque bytes in v1: a generic
LZMA stage can handle all formats, including versions and fields not yet understood
by MSRC. This is not a claim that MSRC validates or parses AVF/MVF/RMV/EVF/RAWVF.
No third-party replay samples are committed or included in distributions.

MSRC 0.2.0 implements the investigated transforms in the Rust core as
[algorithm v2](algorithms/lzma-v2.md), retaining both v1 encoders and decoders.
Its bounded framing readers preserve raw fields and opaque data rather than
using normalized playback events. Source adaptations carry their MIT notices.
The [benchmark reports](benchmarks/README.md) separate replay format versions,
including the subsequently added real RMV 2 samples.

A future format-aware algorithm should first reuse suitable existing parsers and
verify that its transform is invertible over real replay files. Parsing and
reserializing known fields alone does not preserve unknown extensions or signatures.
Any such transform will get its own algorithm version and separate benchmarks.

## Compression implementation

We reuse [lzma-rust2](https://github.com/hasenbanck/lzma-rust2), with
[the documented LZMA reader/writer APIs](https://docs.rs/lzma-rust2/0.24.0/lzma_rust2/),
instead of implementing LZMA. Its official registry metadata lists Rust 1.85 as
the minimum compiler for 0.24.0. We enable `std` and `encoder` only, disabling
its optional optimization, XZ and LZIP features. This keeps a pure Rust codec on
both native and `wasm32-unknown-unknown`, without a C liblzma build or native npm
binary matrix. Integrity checks reuse `crc32fast`.

The API provides memory-limited LZMA construction, strict final-state checking and
recovery of unused buffered compressed input. MSRC applies these to reject large
dictionaries, truncation and trailing stream data. This selection is not a security
audit; native/WASM compilation and independent liblzma interoperability are tested.
There is no need to create a fallback compressor or write a new replay parser for v1.

## Packaging

- Python follows [PyO3/maturin's mixed-project layout](https://www.maturin.rs/project_layout.html)
  with abi3 for CPython 3.10+ and checked-in type stubs.
- npm uses [wasm-pack's Node.js and web targets](https://wasm-bindgen.github.io/wasm-pack/book/commands/build.html),
  packaged together with explicit `@putianyi888/msrc` / `@putianyi888/msrc/web` entry points and generated
  TypeScript declarations. The web target needs initialization before use.
- [Maturin's distribution guidance](https://www.maturin.rs/distribution.html)
  supplies the manylinux wheel strategy used in CI.

Python wheels and the source distribution are distributed through GitHub Releases.
The public JS package is named `@putianyi888/msrc` on npm. Publishing requires
access to that npm scope. Build jobs do not require publishing credentials.
