# Algorithm ID 1, version 2: event columns, delta, ZigZag and LZMA1

Introduced in MSRC 0.2.0. Container version remains 1. This algorithm is the new
compression default; algorithm `(1, 1)` remains available for encoding and decoding.
All transforms below are lossless, fixed-width, and immutable for this tuple.
No varint, event deletion, coordinate clipping, or replay conversion is performed.

## Payload framing

All descriptor integers are unsigned little-endian. The first payload byte is a
mode. It is covered by the outer payload CRC, along with descriptors and LZMA data.

- **Mode 0:** `00 | LZMA-Alone stream`. The stream decodes directly to the original.
- **Mode 1:** `01 | region_count:u32 | descriptors | LZMA-Alone stream`.
  The stream decodes to a transformed buffer of exactly `original_size` bytes.
  Reverse each descriptor's transform to recover the original buffer.
- Other modes are errors. They must not trigger a fallback decoder.

Both modes use the same LZMA-Alone framing as algorithm v1: unknown inner size,
mandatory EOS, strict final-state checking, and no unused compressed bytes.

Each descriptor has the following fields, with no padding:

| Field | Width | Meaning |
| --- | --- | --- |
| layout | u8 | One of the layouts below |
| offset | u64 | Byte offset in the original/transformed buffer |
| length | u64 | Size of the event region in bytes |
| record_count | u32 | Number of on-disk records, not expanded semantic events |
| permutation | 40 or 48 bytes for MVF only | Explicit logical-to-original bit mapping |

The fixed part is 21 bytes. `region_count` and every `record_count` must be positive.
Regions must be in ascending order, nonoverlapping, wholly within `original_size`.
The sum of column widths and counts must equal `length * 8`. Unknown layouts,
record types, invalid permutations, truncated descriptors, and inconsistent
counts or value ranges are errors.

Bytes outside these regions are unchanged, including complete headers, mines,
names, extensions, signatures, terminators, and opaque trailing data. An EVFS
collection uses separate descriptors for its embedded EVFs; each region resets
all predictor baselines to zero.

## Fixed-width arithmetic

For a b-bit absolute field, let `mask = 2^b - 1`. Difference and reconstruction
use modular arithmetic, including on backward times and coordinate wraparound:

```text
encode: difference = (current - previous) & mask; previous = current
decode: current = (previous + difference) & mask; previous = current
```

Time differences are stored as unsigned b-bit integers. Coordinate differences
are interpreted as b-bit two's-complement integers and ZigZag encoded in **b bits**:

```text
0 -> 0, -1 -> 1, +1 -> 2, -2 -> 3, ...
zigzag(d) = ((d << 1) ^ (d >> (b - 1))) & mask  # arithmetic right shift
inverse(z) = ((z >> 1) ^ -(z & 1)) & mask
```

This is the standard [ZigZag mapping](https://protobuf.dev/programming-guides/encoding/#signed-integers),
without Protobuf's subsequent varint step. For example, a 16-bit raw-coordinate
change from 0 to 65535 uses the modular signed difference -1 and ZigZag value 1;
adding the decoded difference modulo 65536 restores 65535 exactly. No extra sign
bit is needed, even for the complete original integer domain.

Fields that already contain time/coordinate increments are not differenced a
second time. Existing signed coordinate increments still receive ZigZag, using
their original width: 16 bits for EVF 0.4, 4 bits per nibble for RMV 2 reduced moves.

## Column storage

Field order is defined per layout below. Values retain the stated bit widths.

For byte-aligned fields, write each value little-endian, then transpose the bytes:
all low bytes, then all second bytes, and so on. For example, the 16-bit sequence
`[2, 1, 0]` becomes `02 01 00 00 00 00`, still six bytes. Each field's planes are
contiguous before the next field begins. This groups zero high bytes for LZMA.

MVF's non-byte-aligned fields are packed as b-bit values, low bit first. Field
columns follow consecutively at the bit level, with no padding between columns.
The total remains exactly 40 or 48 bits per original record.

## Layouts

### 1: AVF

All N records are 8 bytes on disk. Columns: `type:8`, `time:24`, `x:16`, `y:16`.
All counts are N. Preserve every original event code, including aliases.

Read the interleaved coordinates and seconds/hundredths as described in the
[storage investigation](../research/mouse-events/README.md). Before differencing,
represent the stored time as `seconds_field * 100 + hundredths`; this is a 24-bit
centisecond counter, including AVF's stored one-second bias. Hundredths must be
less than 100, and reconstruction must fit the original 16-bit seconds field.
Time uses unsigned differences; x/y use 16-bit differences and ZigZag.
The 8-byte zero-seconds terminator stays outside the region, unchanged.

### 2: RMV 1; 3: RMV 2

The event type column has N bytes. Let M be the number of ordinary mouse records
(`1..7`), B board records (`9..14, 18..27`), C timestamp-change records (`0`, RMV 1
only), and R reduced moves (`28`, RMV 2 only). Columns in order:

| Column | Width | Count | Operation |
| --- | ---: | ---: | --- |
| type | 8 | N | unchanged |
| mouse time | 24 | M | unsigned difference |
| nFlags | 8 | M | unchanged, including all modifier/side-button bits |
| mouse x | 16 | M | difference + ZigZag |
| mouse y | 16 | M | difference + ZigZag |
| board col | 8 | B | difference + ZigZag, independent board-coordinate baseline |
| board row | 8 | B | difference + ZigZag, independent board-coordinate baseline |
| timestamp change | 32 | C | unsigned difference, independent timestamp baseline |
| reduced time increment | 8 | R | already a difference, unchanged |
| reduced packed coordinates | 8 | R | ZigZag each signed 4-bit nibble, x high/y low |

RMV 1's unsigned window coordinates and RMV 2's signed board coordinates retain
their exact 16-bit representations. No UI offset is subtracted and no position
is clipped. A reduced move updates the common mouse time/x/y baselines used by
the next ordinary mouse record. Its implied `nFlags` need no serialized column
entry. The first reduced move requires a preceding ordinary mouse record.

All records are reconstructed in the order of the type column. The termination
code and the remainder of the declared video section stay outside the region.
In particular, real RMV 1 **and RMV 2** samples have extra terminal bytes beyond
the three time bytes listed in the draft; none are discarded or reinterpreted.

### 4: EVF 0.3

N original 8-byte records. Columns: `type:8`, `time:24`, `x:16`, `y:16`, all count N.
Time is the original absolute millisecond counter and uses unsigned differences.
Coordinates use 16-bit differences and ZigZag. Original event codes remain intact.
Terminator and optional checksum stay outside the region.

### 5: EVF 0.4

Supported record types are mouse `1..12`, game state `81,82,83,99`, and pause `255`.
Let M, S, P be their respective counts; N = M + S + P. Columns:

| Column | Width | Count | Operation |
| --- | ---: | ---: | --- |
| type | 8 | N | unchanged |
| timed event increment | 8 | M + S | already a difference, unchanged |
| mouse x increment | 16 | M | ZigZag the original signed increment |
| mouse y increment | 16 | M | ZigZag the original signed increment |
| pause increment | 16 | P | already a difference, unchanged |

The initial mouse position is also passed through the same 16-bit mapping;
its original representation is restored exactly. Game-state and pause records
remain in their original places. Pause splitting, counts and payloads are never
regenerated from normalized times. Terminator, checksum length and checksum remain
unchanged outside the event region.

The currently conflicting upstream board/metric layouts are not guessed. An
encoder unable to recognize a complete event stream uses mode 0 for that file.

### 6: MVF 0.97; 7: MVF 2006/2007

First undo the file's bit permutation. Store the mapping explicitly in the
descriptor: `permutation[j]` is the original record bit containing logical bit j.
Original record bits are numbered low bit first within each byte. Every integer
from 0 through 39 (or 47) must occur exactly once. Decoding never needs sin/cos or
floating-point seed reconstruction; the seed itself remains in the original header.

| Column | MVF 0.97 width | MVF 2006/2007 width | Operation |
| --- | ---: | ---: | --- |
| buttons | 3 | 3 | exact state snapshot, unchanged |
| time | 17 | 22 | unsigned difference |
| x | 9 | 11 | difference + same-width ZigZag |
| y | 9 | 11 | difference + same-width ZigZag |
| residual bits | 2 | 1 | unchanged, even when nonzero |

All columns have N values. In 0.97 the time becomes `seconds * 100 + hundredths`
before differencing, with hundredths less than 100 and reconstructed seconds at
most 1023. In 2006/2007 the time is the original absolute millisecond field.
Coordinate values are the original packed values; the newer format's stored
32-pixel bias is preserved. This retains the complete raw coordinate domain.

Reverse the field transforms, rebuild the complete logical record including
residual bits, and apply the saved inverse permutation. No snapshot is expanded
into click/move events. The declared record-count boundary and any trailer remain
unchanged. Earlier MVF versions use mode 0 in this encoder.

## Encoder policy and resource limits

The current encoder recognizes bounded regions in AVF, MVF 0.97/2006/2007, RMV 1/2,
EVF 0.3/0.4, and EVFS 0.0. Recognition is not general replay validation. Unknown
versions/events, noncanonical split times, and framing errors use mode 0.

For recognized inputs it compares the complete mode-1 payload against mode 0 at
the same LZMA preset and chooses the smaller, preferring mode 0 on ties. Thus this
encoder's v2 archive is at most one byte larger than its v1 archive at the same
preset. This size comparison costs an additional LZMA encode on recognized files;
it is included in the [benchmark](../benchmarks/README.md), not timed away.

Transformed data has exactly the original size, so LZMA expansion is bounded by
the same outer output limit. Descriptors are checked without reserving an array
from their declared count. Decoding retains the transformed output and temporary
storage for one event region and its type column. `max_memory_kib` limits LZMA's
working-memory estimate, not total process memory or this preprocessing scratch.
Original CRC validation happens after every inverse transform, before returning.

New layouts or inverse operations require a new algorithm version or ID. Encoder
recognition/preset/size-selection tuning is allowed only when it emits the same
documented wire format. New algorithms must retain compression and decompression
for every existing version.

## Independent fixtures

`tests/generate_v2_fixtures.py` builds synthetic EVF 0.3 and 0.4 originals, performs
an independent Python fixed-width transform, and uses standard-library liblzma to
construct the complete v2 envelopes. Both archive and original hex files are
committed under `tests/fixtures/lzma-v2-*`. Rust, Python, Node.js and web ESM tests
decode them and exercise both retained encoders. The v1 fixture remains unchanged.
