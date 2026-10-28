//! LZMA algorithm v2: lossless fixed-width event columns, delta and ZigZag.
//! Upstream framing/permutation attribution: LICENSE-THIRD-PARTY in this directory.

mod columns;
mod parse;
#[cfg(test)]
mod tests;

use crate::{DecompressionLimits, Error, Result, lzma};
use columns::{Columns, delta, mask, unzigzag, zeroed, zigzag};
use parse::Reader;

fn invalid(reason: &'static str) -> Error {
    Error::InvalidPreprocessing(reason)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Kind {
    Avf = 1,
    Rmv1 = 2,
    Rmv2 = 3,
    Evf3 = 4,
    Evf4 = 5,
    Mvf097 = 6,
    Mvf2007 = 7,
}

impl Kind {
    fn from_byte(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Avf),
            2 => Ok(Self::Rmv1),
            3 => Ok(Self::Rmv2),
            4 => Ok(Self::Evf3),
            5 => Ok(Self::Evf4),
            6 => Ok(Self::Mvf097),
            7 => Ok(Self::Mvf2007),
            _ => Err(invalid("unsupported event layout")),
        }
    }
    fn permutation_size(self) -> usize {
        match self {
            Self::Mvf097 => 40,
            Self::Mvf2007 => 48,
            _ => 0,
        }
    }
    fn record_size(self, code: u8) -> Result<usize> {
        match self {
            Self::Avf if matches!(code, 1 | 3 | 5 | 9 | 11 | 17 | 21 | 33 | 65 | 145 | 193) => {
                Ok(8)
            }
            Self::Evf3 if (1..=12).contains(&code) => Ok(8),
            Self::Rmv1 | Self::Rmv2 => match code {
                1..=7 => Ok(9),
                9..=14 | 18..=27 => Ok(3),
                0 if self == Self::Rmv1 => Ok(5),
                28 if self == Self::Rmv2 => Ok(3),
                _ => Err(invalid("unsupported RMV event")),
            },
            Self::Evf4 => match code {
                1..=12 => Ok(6),
                81..=83 | 99 => Ok(2),
                255 => Ok(3),
                _ => Err(invalid("unsupported EVF event")),
            },
            _ => Err(invalid("unsupported event code")),
        }
    }
}

#[derive(Debug)]
struct Region {
    kind: Kind,
    offset: usize,
    length: usize,
    records: usize,
    permutation: Vec<u8>,
}

/// The plain candidate also gives unknown inputs a stable v2 decoding procedure.
/// Selecting the smaller candidate is encoder policy, not a different algorithm.
pub(crate) fn encode(input: &[u8], level: u32) -> Result<Vec<u8>> {
    let mut plain = vec![0];
    plain.extend_from_slice(&lzma::encode(input, level)?);
    let regions = match parse::find(input) {
        Ok(regions) => regions
            .into_iter()
            .filter(|r| r.records != 0)
            .collect::<Vec<_>>(),
        Err(_) => return Ok(plain),
    };
    if regions.is_empty()
        || regions.len() > u32::MAX as usize
        || regions.iter().any(|r| r.records > u32::MAX as usize)
    {
        return Ok(plain);
    }
    let mut transformed = zeroed(input.len())?;
    transformed.copy_from_slice(input);
    let mut payload = vec![1];
    payload.extend_from_slice(&(regions.len() as u32).to_le_bytes());
    for region in &regions {
        let end = region
            .offset
            .checked_add(region.length)
            .ok_or_else(|| invalid("region size overflow"))?;
        let converted = match transform(region, &input[region.offset..end], false) {
            Ok(value) => value,
            Err(Error::InvalidPreprocessing(_)) => return Ok(plain),
            Err(error) => return Err(error),
        };
        transformed[region.offset..end].copy_from_slice(&converted);
        payload.push(region.kind as u8);
        payload.extend_from_slice(&(region.offset as u64).to_le_bytes());
        payload.extend_from_slice(&(region.length as u64).to_le_bytes());
        payload.extend_from_slice(&(region.records as u32).to_le_bytes());
        payload.extend_from_slice(&region.permutation);
    }
    payload.extend_from_slice(&lzma::encode(&transformed, level)?);
    Ok(if payload.len() < plain.len() {
        payload
    } else {
        plain
    })
}

pub(crate) fn decode(payload: &[u8], size: u64, limits: DecompressionLimits) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    match r.uint(1)? {
        0 => lzma::decode(&payload[1..], size, limits),
        1 => {
            let count = u32::from_le_bytes(r.take(4)?.try_into().unwrap()) as usize;
            if count == 0 || count > payload.len() / 21 || count as u64 > size {
                return Err(invalid("invalid event region count"));
            }
            let mut previous_end = 0u64;
            // Validate descriptors without allocating from a declared count.
            // Re-read them after LZMA decoding, keeping only one region in memory.
            for _ in 0..count {
                read_region(&mut r, size, &mut previous_end)?;
            }
            let mut output = lzma::decode(&payload[r.pos..], size, limits)?;
            r.pos = 5;
            previous_end = 0;
            for _ in 0..count {
                let region = read_region(&mut r, size, &mut previous_end)?;
                let end = region.offset + region.length;
                let restored = transform(&region, &output[region.offset..end], true)?;
                output[region.offset..end].copy_from_slice(&restored);
            }
            Ok(output)
        }
        _ => Err(invalid("unsupported preprocessing mode")),
    }
}

fn read_region(r: &mut Reader<'_>, size: u64, previous_end: &mut u64) -> Result<Region> {
    let kind = Kind::from_byte(r.uint(1)? as u8)?;
    let offset = u64::from_le_bytes(r.take(8)?.try_into().unwrap());
    let length = u64::from_le_bytes(r.take(8)?.try_into().unwrap());
    let records = u32::from_le_bytes(r.take(4)?.try_into().unwrap()) as usize;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| invalid("region size overflow"))?;
    if offset < *previous_end || end > size || records == 0 || length < records as u64 {
        return Err(invalid("overlapping or out-of-bounds event region"));
    }
    *previous_end = end;
    let permutation = r.take(kind.permutation_size())?.to_vec();
    let mut seen = 0u64;
    for &bit in &permutation {
        if bit as usize >= permutation.len() || seen & (1u64 << bit) != 0 {
            return Err(invalid("invalid MVF permutation"));
        }
        seen |= 1u64 << bit;
    }
    Ok(Region {
        kind,
        offset: usize::try_from(offset).map_err(|_| Error::AllocationFailed)?,
        length: usize::try_from(length).map_err(|_| Error::AllocationFailed)?,
        records,
        permutation,
    })
}

fn transform(region: &Region, input: &[u8], inverse: bool) -> Result<Vec<u8>> {
    if matches!(region.kind, Kind::Mvf097 | Kind::Mvf2007) {
        return transform_mvf(region, input, inverse);
    }
    let mut codes = Vec::new();
    codes
        .try_reserve_exact(region.records)
        .map_err(|_| Error::AllocationFailed)?;
    let mut pos = 0usize;
    for _ in 0..region.records {
        let code = *input
            .get(pos)
            .ok_or_else(|| invalid("truncated event type column"))?;
        codes.push(code);
        pos = pos
            .checked_add(if inverse {
                1
            } else {
                region.kind.record_size(code)?
            })
            .ok_or_else(|| invalid("event size overflow"))?;
    }
    if !inverse && pos != input.len() {
        return Err(invalid("event count mismatch"));
    }
    let n = region.records;
    let specs = match region.kind {
        Kind::Avf | Kind::Evf3 => vec![(8, n), (24, n), (16, n), (16, n)],
        Kind::Rmv1 | Kind::Rmv2 => {
            let mouse = codes.iter().filter(|&&c| (1..=7).contains(&c)).count();
            let board = codes
                .iter()
                .filter(|&&c| matches!(c, 9..=14 | 18..=27))
                .count();
            let timestamp = codes.iter().filter(|&&c| c == 0).count();
            let reduced = codes.iter().filter(|&&c| c == 28).count();
            vec![
                (8, n),
                (24, mouse),
                (8, mouse),
                (16, mouse),
                (16, mouse),
                (8, board),
                (8, board),
                (32, timestamp),
                (8, reduced),
                (8, reduced),
            ]
        }
        Kind::Evf4 => {
            let mouse = codes.iter().filter(|&&c| (1..=12).contains(&c)).count();
            let state = codes.iter().filter(|&&c| matches!(c, 81..=83 | 99)).count();
            let pause = codes.iter().filter(|&&c| c == 255).count();
            vec![
                (8, n),
                (8, mouse + state),
                (16, mouse),
                (16, mouse),
                (16, pause),
            ]
        }
        _ => unreachable!(),
    };
    let mut columns = Columns::new(&specs, input.len())?;
    let mut output = zeroed(input.len())?;
    let mut pos = 0usize;
    let (mut t, mut x, mut y, mut bx, mut by, mut timestamp) = (0, 0, 0, 0, 0, 0);
    let mut mouse_seen = false;
    for code in codes {
        let width = region.kind.record_size(code)?;
        let end = pos
            .checked_add(width)
            .filter(|&end| end <= input.len())
            .ok_or_else(|| invalid("event runs past region"))?;
        let source = &input[pos..end];
        let mut record = [0u8; 9];
        let target = &mut record[..width];
        if inverse {
            columns.read(input, 0)?;
            target[0] = code;
        } else {
            columns.write(&mut output, 0, code as u32)?;
        }
        // Field access is shared by encode/decode; output is columns on encode
        // and reconstructed on-disk records on decode.
        match region.kind {
            Kind::Avf | Kind::Evf3 => {
                let avf = region.kind == Kind::Avf;
                let mut values = if inverse {
                    [
                        columns.read(input, 1)?,
                        columns.read(input, 2)?,
                        columns.read(input, 3)?,
                    ]
                } else if avf {
                    if source[4] >= 100 {
                        return Err(invalid("noncanonical AVF hundredths"));
                    }
                    [
                        (((source[6] as u32) << 8) | source[2] as u32) * 100 + source[4] as u32,
                        ((source[1] as u32) << 8) | source[3] as u32,
                        ((source[5] as u32) << 8) | source[7] as u32,
                    ]
                } else {
                    [be(&source[1..4]), be(&source[4..6]), be(&source[6..8])]
                };
                values[0] = delta(values[0], &mut t, 24, false, inverse);
                values[1] = delta(values[1], &mut x, 16, true, inverse);
                values[2] = delta(values[2], &mut y, 16, true, inverse);
                if inverse {
                    if avf {
                        if t / 100 > u16::MAX as u32 {
                            return Err(invalid("AVF time out of range"));
                        }
                        let seconds = (t / 100) as u16;
                        target[1] = (x >> 8) as u8;
                        target[2] = seconds as u8;
                        target[3] = x as u8;
                        target[4] = (t % 100) as u8;
                        target[5] = (y >> 8) as u8;
                        target[6] = (seconds >> 8) as u8;
                        target[7] = y as u8;
                    } else {
                        put_be(&mut target[1..4], t);
                        put_be(&mut target[4..6], x);
                        put_be(&mut target[6..8], y);
                    }
                } else {
                    for (i, value) in values.into_iter().enumerate() {
                        columns.write(&mut output, i + 1, value)?;
                    }
                }
            }
            Kind::Rmv1 | Kind::Rmv2 => match code {
                1..=7 => {
                    let mut values = if inverse {
                        [
                            columns.read(input, 1)?,
                            columns.read(input, 2)?,
                            columns.read(input, 3)?,
                            columns.read(input, 4)?,
                        ]
                    } else {
                        [
                            be(&source[1..4]),
                            source[4] as u32,
                            be(&source[5..7]),
                            be(&source[7..9]),
                        ]
                    };
                    values[0] = delta(values[0], &mut t, 24, false, inverse);
                    values[2] = delta(values[2], &mut x, 16, true, inverse);
                    values[3] = delta(values[3], &mut y, 16, true, inverse);
                    mouse_seen = true;
                    if inverse {
                        put_be(&mut target[1..4], t);
                        target[4] = values[1] as u8;
                        put_be(&mut target[5..7], x);
                        put_be(&mut target[7..9], y);
                    } else {
                        for (i, value) in values.into_iter().enumerate() {
                            columns.write(&mut output, i + 1, value)?;
                        }
                    }
                }
                9..=14 | 18..=27 => {
                    for (field, index, previous) in [(5, 1, &mut bx), (6, 2, &mut by)] {
                        let v = if inverse {
                            columns.read(input, field)?
                        } else {
                            source[index] as u32
                        };
                        let v = delta(v, previous, 8, true, inverse);
                        if inverse {
                            target[index] = v as u8;
                        } else {
                            columns.write(&mut output, field, v)?;
                        }
                    }
                }
                0 => {
                    let v = if inverse {
                        columns.read(input, 7)?
                    } else {
                        be(&source[1..5])
                    };
                    let v = delta(v, &mut timestamp, 32, false, inverse);
                    if inverse {
                        put_be(&mut target[1..5], v);
                    } else {
                        columns.write(&mut output, 7, v)?;
                    }
                }
                28 => {
                    if !mouse_seen {
                        return Err(invalid("RMV reduced move before initial position"));
                    }
                    let dt = if inverse {
                        columns.read(input, 8)?
                    } else {
                        source[1] as u32
                    };
                    let packed = if inverse {
                        columns.read(input, 9)?
                    } else {
                        source[2] as u32
                    };
                    let nibble = |n| {
                        if inverse {
                            unzigzag(n, 4)
                        } else {
                            zigzag(n, 4)
                        }
                    };
                    let converted = (nibble(packed >> 4) << 4) | nibble(packed & 15);
                    let original = if inverse { converted } else { packed };
                    let signed4 = |n: u32| (n & 7).wrapping_sub(n & 8);
                    t = t.wrapping_add(dt) & mask(24);
                    x = x.wrapping_add(signed4(original >> 4)) & mask(16);
                    y = y.wrapping_add(signed4(original & 15)) & mask(16);
                    if inverse {
                        target[1] = dt as u8;
                        target[2] = converted as u8;
                    } else {
                        columns.write(&mut output, 8, dt)?;
                        columns.write(&mut output, 9, converted)?;
                    }
                }
                _ => return Err(invalid("unsupported RMV event")),
            },
            Kind::Evf4 => match code {
                1..=12 => {
                    let dt = if inverse {
                        columns.read(input, 1)?
                    } else {
                        source[1] as u32
                    };
                    let convert = |v| {
                        if inverse {
                            unzigzag(v, 16)
                        } else {
                            zigzag(v, 16)
                        }
                    };
                    let dx = convert(if inverse {
                        columns.read(input, 2)?
                    } else {
                        be(&source[2..4])
                    });
                    let dy = convert(if inverse {
                        columns.read(input, 3)?
                    } else {
                        be(&source[4..6])
                    });
                    if inverse {
                        target[1] = dt as u8;
                        put_be(&mut target[2..4], dx);
                        put_be(&mut target[4..6], dy);
                    } else {
                        columns.write(&mut output, 1, dt)?;
                        columns.write(&mut output, 2, dx)?;
                        columns.write(&mut output, 3, dy)?;
                    }
                }
                81..=83 | 99 => {
                    let dt = if inverse {
                        columns.read(input, 1)?
                    } else {
                        source[1] as u32
                    };
                    if inverse {
                        target[1] = dt as u8;
                    } else {
                        columns.write(&mut output, 1, dt)?;
                    }
                }
                255 => {
                    let dt = if inverse {
                        columns.read(input, 4)?
                    } else {
                        be(&source[1..3])
                    };
                    if inverse {
                        put_be(&mut target[1..3], dt);
                    } else {
                        columns.write(&mut output, 4, dt)?;
                    }
                }
                _ => return Err(invalid("unsupported EVF event")),
            },
            _ => unreachable!(),
        }
        if inverse {
            output[pos..end].copy_from_slice(target);
        }
        pos = end;
    }
    if pos != input.len() {
        return Err(invalid("event count mismatch"));
    }
    columns.finish()?;
    Ok(output)
}

fn be(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0, |n, &b| (n << 8) | b as u32)
}
fn put_be(bytes: &mut [u8], value: u32) {
    let length = bytes.len();
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = (value >> (8 * (length - i - 1))) as u8;
    }
}

fn mvf_logical(record: &[u8], permutation: &[u8]) -> u64 {
    permutation.iter().enumerate().fold(0, |n, (i, &p)| {
        n | ((((record[p as usize / 8] >> (p % 8)) & 1) as u64) << i)
    })
}

fn transform_mvf(region: &Region, input: &[u8], inverse: bool) -> Result<Vec<u8>> {
    let old = region.kind == Kind::Mvf097;
    let (width, tbits, cbits, rbits) = if old { (5, 17, 9, 2) } else { (6, 22, 11, 1) };
    if region.records.checked_mul(width) != Some(input.len()) {
        return Err(invalid("MVF event count mismatch"));
    }
    let n = region.records;
    let mut columns = Columns::new(
        &[(3, n), (tbits, n), (cbits, n), (cbits, n), (rbits, n)],
        input.len(),
    )?;
    let mut output = zeroed(input.len())?;
    let (mut t, mut x, mut y) = (0, 0, 0);
    for row in 0..n {
        let mut values = if inverse {
            [
                columns.read(input, 0)?,
                columns.read(input, 1)?,
                columns.read(input, 2)?,
                columns.read(input, 3)?,
                columns.read(input, 4)?,
            ]
        } else {
            let raw = mvf_logical(&input[row * width..(row + 1) * width], &region.permutation);
            if old {
                let cs = ((raw >> 21) & 127) as u32;
                if cs >= 100 {
                    return Err(invalid("noncanonical MVF hundredths"));
                }
                [
                    (raw & 7) as u32,
                    ((raw >> 28) & 1023) as u32 * 100 + cs,
                    ((raw >> 12) & 511) as u32,
                    ((raw >> 3) & 511) as u32,
                    (raw >> 38) as u32,
                ]
            } else {
                [
                    (raw & 7) as u32,
                    ((raw >> 25) & ((1 << 22) - 1)) as u32,
                    ((raw >> 14) & 2047) as u32,
                    ((raw >> 3) & 2047) as u32,
                    (raw >> 47) as u32,
                ]
            }
        };
        values[1] = delta(values[1], &mut t, tbits, false, inverse);
        values[2] = delta(values[2], &mut x, cbits, true, inverse);
        values[3] = delta(values[3], &mut y, cbits, true, inverse);
        if inverse {
            let raw = if old {
                if t / 100 > 1023 {
                    return Err(invalid("MVF time out of range"));
                }
                values[0] as u64
                    | ((y as u64) << 3)
                    | ((x as u64) << 12)
                    | (((t % 100) as u64) << 21)
                    | (((t / 100) as u64) << 28)
                    | ((values[4] as u64) << 38)
            } else {
                values[0] as u64
                    | ((y as u64) << 3)
                    | ((x as u64) << 14)
                    | ((t as u64) << 25)
                    | ((values[4] as u64) << 47)
            };
            for (bit, &p) in region.permutation.iter().enumerate() {
                output[row * width + p as usize / 8] |= (((raw >> bit) & 1) as u8) << (p % 8);
            }
        } else {
            for (i, v) in values.into_iter().enumerate() {
                columns.write(&mut output, i, v)?;
            }
        }
    }
    columns.finish()?;
    Ok(output)
}
