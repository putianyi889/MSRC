use crate::{Error, Result};

use super::invalid;

/// Fixed-width columns, byte-shuffled when their widths and offsets are aligned.
/// Non-byte-aligned MVF fields are packed continuously without padding.
pub(super) struct Columns {
    fields: Vec<Field>,
    bytes: usize,
}

struct Field {
    width: u8,
    base: usize,
    count: usize,
    cursor: usize,
}

impl Columns {
    pub fn new(specs: &[(u8, usize)], bytes: usize) -> Result<Self> {
        let mut base = 0usize;
        let mut fields = Vec::new();
        for &(width, count) in specs {
            if width == 0 || width > 32 {
                return Err(invalid("invalid column width"));
            }
            fields.push(Field {
                width,
                base,
                count,
                cursor: 0,
            });
            base = base
                .checked_add(
                    count
                        .checked_mul(width as usize)
                        .ok_or_else(|| invalid("column size overflow"))?,
                )
                .ok_or_else(|| invalid("column size overflow"))?;
        }
        if bytes.checked_mul(8) != Some(base) {
            return Err(invalid("column sizes do not match the event region"));
        }
        Ok(Self { fields, bytes })
    }

    pub fn read(&mut self, input: &[u8], field: usize) -> Result<u32> {
        if input.len() != self.bytes {
            return Err(invalid("invalid column input length"));
        }
        let f = &mut self.fields[field];
        if f.cursor >= f.count {
            return Err(invalid("column exhausted"));
        }
        let mut value = 0;
        if f.width % 8 == 0 && f.base % 8 == 0 {
            for byte in 0..f.width as usize / 8 {
                value |= (input[f.base / 8 + byte * f.count + f.cursor] as u32) << (byte * 8);
            }
        } else {
            let start = f.base + f.cursor * f.width as usize;
            for bit in 0..f.width as usize {
                value |= (((input[(start + bit) / 8] >> ((start + bit) % 8)) & 1) as u32) << bit;
            }
        }
        f.cursor += 1;
        Ok(value)
    }

    pub fn write(&mut self, output: &mut [u8], field: usize, value: u32) -> Result<()> {
        if output.len() != self.bytes {
            return Err(invalid("invalid column output length"));
        }
        let f = &mut self.fields[field];
        if f.cursor >= f.count || value & !mask(f.width) != 0 {
            return Err(invalid("column value out of range"));
        }
        if f.width % 8 == 0 && f.base % 8 == 0 {
            for byte in 0..f.width as usize / 8 {
                output[f.base / 8 + byte * f.count + f.cursor] = (value >> (byte * 8)) as u8;
            }
        } else {
            let start = f.base + f.cursor * f.width as usize;
            for bit in 0..f.width as usize {
                output[(start + bit) / 8] |= (((value >> bit) & 1) as u8) << ((start + bit) % 8);
            }
        }
        f.cursor += 1;
        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        if self.fields.iter().any(|f| f.cursor != f.count) {
            return Err(invalid("unused column values"));
        }
        Ok(())
    }
}

pub(super) fn zeroed(size: usize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(size)
        .map_err(|_| Error::AllocationFailed)?;
    output.resize(size, 0);
    Ok(output)
}

pub(super) const fn mask(bits: u8) -> u32 {
    u32::MAX >> (32 - bits)
}

pub(super) fn zigzag(value: u32, bits: u8) -> u32 {
    ((value << 1) ^ 0u32.wrapping_sub(value >> (bits - 1))) & mask(bits)
}

pub(super) fn unzigzag(value: u32, bits: u8) -> u32 {
    ((value >> 1) ^ 0u32.wrapping_sub(value & 1)) & mask(bits)
}

pub(super) fn delta(value: u32, previous: &mut u32, bits: u8, signed: bool, inverse: bool) -> u32 {
    if inverse {
        let difference = if signed { unzigzag(value, bits) } else { value };
        *previous = previous.wrapping_add(difference) & mask(bits);
        *previous
    } else {
        let difference = value.wrapping_sub(*previous) & mask(bits);
        *previous = value;
        if signed {
            zigzag(difference, bits)
        } else {
            difference
        }
    }
}
