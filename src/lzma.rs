use std::io::{Cursor, Read, Write};

use lzma_rust2::{LzmaOptions, LzmaReader, LzmaWriter};

use crate::{DecompressionLimits, Error, Result};

pub(crate) fn encode(input: &[u8], level: u32) -> Result<Vec<u8>> {
    let mut options = LzmaOptions::with_preset(level);
    // Replays are usually small. Avoid allocating a full preset dictionary for
    // short input; the selected dictionary is embedded in the .lzma header.
    options.dict_size = input.len().clamp(4096, 8 * 1024 * 1024).next_power_of_two() as u32;
    // Unknown inner size + mandatory end marker prevents early, size-only EOF.
    let mut writer = LzmaWriter::new(Vec::new(), &options, true, true, None)?;
    writer.write_all(input)?;
    Ok(writer.finish()?)
}

pub(crate) fn decode(
    payload: &[u8],
    original_size: u64,
    limits: DecompressionLimits,
) -> Result<Vec<u8>> {
    if payload.len() < 13 {
        return Err(Error::InvalidLzmaFraming("missing .lzma header"));
    }
    if payload[5..13] != [0xff; 8] {
        return Err(Error::InvalidLzmaFraming(
            "inner size must be unknown; end marker required",
        ));
    }
    let mut reader = LzmaReader::new_mem_limit(Cursor::new(payload), limits.max_memory_kib, None)?;
    reader.set_relaxed_end_condition(false);
    let mut output = Vec::new();
    // Incremental bounded reads: a dishonest size never triggers an allocation
    // of that size, nor can the decoder expand beyond it unnoticed.
    let mut buffer = [0; 8192];
    loop {
        let remaining = original_size.saturating_sub(output.len() as u64);
        let request = remaining.min((buffer.len() - 1) as u64) as usize + 1;
        let count = reader.read(&mut buffer[..request])?;
        if count == 0 {
            break;
        }
        if count as u64 > remaining {
            return Err(Error::OutputSizeMismatch);
        }
        output
            .try_reserve(count)
            .map_err(|_| Error::AllocationFailed)?;
        output.extend_from_slice(&buffer[..count]);
    }
    if output.len() as u64 != original_size {
        return Err(Error::OutputSizeMismatch);
    }
    let (cursor, buffered) = reader.into_parts();
    if !buffered.is_empty() || cursor.position() != payload.len() as u64 {
        return Err(Error::InvalidLzmaFraming(
            "trailing bytes after the LZMA stream",
        ));
    }
    Ok(output)
}
