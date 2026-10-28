# MSRC container v1

Status: implemented by MSRC 0.1.0 onward; algorithm v2 is added in 0.2.0.
Suggested extension: `.msrc`. Each archive restores exactly one original file.
The header stores no filename, path or filesystem metadata. Algorithm v2 payloads
may identify event layouts for reversible preprocessing; no replay normalization
is performed. All multibyte header integers are unsigned and little-endian.

## Fixed header (40 bytes)

| Offset | Size | Field | Value / meaning |
| --- | --- | --- | --- |
| 0 | 4 | Magic | ASCII `MSRC` (`4d 53 52 43`) |
| 4 | 2 | Container version | `1` |
| 6 | 2 | Algorithm ID | `1` = LZMA1 |
| 8 | 2 | Algorithm version | `1` or `2`, as specified below |
| 10 | 2 | Flags | Must be zero in container v1 |
| 12 | 8 | Original size | Exact number of original bytes |
| 20 | 8 | Payload size | Exact number of bytes following the header |
| 28 | 4 | Original CRC-32 | CRC of the complete original file |
| 32 | 4 | Payload CRC-32 | CRC of all payload bytes |
| 36 | 4 | Header CRC-32 | CRC of bytes at offsets 0 through 35 |
| 40 | variable | Payload | Exactly `payload_size` bytes |

All CRC fields use CRC-32/ISO-HDLC (the ZIP/IEEE CRC-32 used by `zlib.crc32`
and `crc32fast`; reflected polynomial `0xedb88320`, initial and final XOR
`0xffffffff`). Empty input has CRC zero. CRCs detect accidental damage; they do
not establish authenticity. No alignment padding or trailing bytes are permitted.
Container concatenation and multi-file archives are not supported.

## Algorithm ID 1, version 1: LZMA1

The payload is exactly one LZMA-Alone (`.lzma`) stream, not XZ, LZMA2 or 7z:

1. One LZMA properties byte.
2. Four-byte little-endian dictionary size.
3. Eight bytes `ff ff ff ff ff ff ff ff`, denoting unknown inner output size.
4. LZMA1 range-coded data with a mandatory end-of-stream marker.

The decoder must read through the end marker, validate the range coder's final
state and reject unused compressed bytes, even when size and CRC fields agree.
The output size must exactly equal the outer `original_size`. Known-size inner
streams are deliberately excluded, so they cannot bypass end-marker validation.

The current encoder defaults to preset 6. It selects a dictionary equal to the
next power of two of the original size clamped to [4096, 8388608]. Dictionaries,
properties and compression presets are carried by the `.lzma` stream and do not
change the algorithm version. Conforming independent encoders may choose other
valid LZMA1 settings; decoders may reject them when their configured memory limit
would be exceeded. Compressed bytes need not be identical across encoder versions.

No lossy event reduction, delta transform, board reconstruction, reordering or
conversion to EVF/RAWVF is performed. The result must reproduce every input byte,
including opaque extensions, signatures and padding.

## Algorithm ID 1, version 2: fixed-width preprocessing + LZMA1

The [complete v2 specification](algorithms/lzma-v2.md) defines a mode byte,
optional event-region descriptors, and one strict LZMA-Alone stream. Known event
fields are separated into columns, absolute times/coordinates differenced, and
signed coordinate differences ZigZag encoded without changing field widths.
Already-delta EVF coordinates and RMV reduced moves also receive ZigZag.
The transformed buffer has exactly `original_size` bytes; inverse preprocessing
restores all original bits before CRC verification. Unknown formats use the
documented mode-0 raw path. Compression defaults to v2 starting in MSRC 0.2.0.

## Version policy

- Container version describes header/framing semantics. Incompatible header
  changes require a new container version; version 1 cannot gain nonzero flags.
- Algorithm ID identifies the codec family. IDs are assigned in this document;
  `0` is reserved and not a passthrough mode.
- Algorithm version identifies the decoding procedure, including any future
  replay-specific preprocessing. New transforms require a new version or ID.
- Existing `(container version, algorithm ID, algorithm version)` tuples are
  immutable. New releases retain old encoders, decoders and checked-in compatibility
  data. Both versions remain explicitly selectable for compression in every binding.
- Package SemVer is independent. Encoder tuning or adding an algorithm does not
  invalidate files written by earlier versions. Changing defaults only affects
  newly compressed data.
- Unknown container versions, flags or decoder tuples fail explicitly. There is
  no guessing or fallback to a different decoder. `inspect` can report unknown
  algorithm tuples within a supported container version without decoding them.

## Decode limits

Default output limit: 67108864 bytes. Default LZMA memory estimate limit: 65536 KiB.
Inspect validates header CRC, exact payload length and payload CRC. Decode also
checks supported algorithm, declared output limit, inner framing, LZMA memory
estimate, actual output size, full stream termination, and original CRC.
Algorithm v2 also validates its descriptors and reverses preprocessing; its
temporary region/type-column buffers are additional to the LZMA memory estimate.
Output is read incrementally and stopped if it exceeds the declared size; a forged
size does not cause preallocation of the declared amount. Callers own input
allocation and execution time policies. A decoder memory limit does not account
for caller buffers, binding copies, output capacity or total process overhead.

## Independent fixtures

`tests/fixtures/lzma-v1.hex` is a complete archive generated by
`tests/generate_fixture.py` using Python's standard `lzma`, `struct` and `zlib`
modules, without MSRC. Its original bytes are ASCII
`MSRC interoperability fixture\n` followed by the byte sequence 0..255 repeated
four times. This is synthetic data, not a replay. Rust, Python and JavaScript tests
all decode it. The Python tests also decode Rust-encoded payloads with liblzma
through Python's standard library.

`tests/fixtures/lzma-v2-*` adds independently generated EVF 0.3/0.4 transformed
archives and originals; see the [v2 fixture definition](algorithms/lzma-v2.md#independent-fixtures).
