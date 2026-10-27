use wasm_bindgen::prelude::*;

// Accept JS numbers explicitly to reject fractions, NaN, infinity and negative
// values instead of allowing JS-to-Wasm integer coercions to silently wrap them.
fn integer(value: f64, max: u64, name: &str) -> Result<u64, JsError> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > max as f64 {
        return Err(JsError::new(&format!(
            "{name} must be an integer in 0..={max}"
        )));
    }
    Ok(value as u64)
}

#[wasm_bindgen]
pub fn compress(data: &[u8], level: Option<f64>) -> Result<Vec<u8>, JsError> {
    let level = integer(level.unwrap_or(6.0), 9, "level")? as u32;
    Ok(msrc::compress_with_options(
        data,
        msrc::CompressionOptions {
            level,
            ..Default::default()
        },
    )?)
}

#[wasm_bindgen(js_name = decompress)]
pub fn decompress(
    data: &[u8],
    max_output_size: Option<f64>,
    max_memory_kib: Option<f64>,
) -> Result<Vec<u8>, JsError> {
    let limits = msrc::DecompressionLimits {
        max_output_size: integer(
            max_output_size.unwrap_or(msrc::DEFAULT_MAX_OUTPUT_SIZE as f64),
            u32::MAX as u64,
            "max_output_size",
        )?,
        max_memory_kib: integer(
            max_memory_kib.unwrap_or(msrc::DEFAULT_MAX_MEMORY_KIB as f64),
            u32::MAX as u64,
            "max_memory_kib",
        )? as u32,
    };
    Ok(msrc::decompress_with_limits(data, limits)?)
}

/// Free with .free() after use; 64-bit sizes are JavaScript bigint values.
#[wasm_bindgen]
pub struct ArchiveInfo {
    pub container_version: u16,
    pub algorithm_id: u16,
    pub algorithm_version: u16,
    pub original_size: u64,
    pub payload_size: u64,
    pub original_crc32: u32,
    pub payload_crc32: u32,
}

#[wasm_bindgen]
pub fn inspect(data: &[u8]) -> Result<ArchiveInfo, JsError> {
    let header = msrc::inspect(data)?;
    Ok(ArchiveInfo {
        container_version: header.container_version,
        algorithm_id: header.algorithm_id,
        algorithm_version: header.algorithm_version,
        original_size: header.original_size,
        payload_size: header.payload_size,
        original_crc32: header.original_crc32,
        payload_crc32: header.payload_crc32,
    })
}
