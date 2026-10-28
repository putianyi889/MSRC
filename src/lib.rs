//! Lossless compression of Minesweeper replay bytes in a versioned MSRC container.
//!
//! LZMA v2 adds lossless fixed-width replay event preprocessing; LZMA v1 remains
//! available for both compression and decompression.
//! Container, algorithm and package versions are independent.
//!
//! ```
//! let replay = b"original replay bytes";
//! let archive = msrc::compress(replay)?;
//! assert_eq!(msrc::decompress(&archive)?, replay);
//! assert_eq!(msrc::inspect(&archive)?.algorithm()?, msrc::Algorithm::LzmaV2);
//! # Ok::<(), msrc::Error>(())
//! ```

#![forbid(unsafe_code)]

mod error;
mod header;
mod lzma;
mod preprocess;

pub use error::{Error, Result};
pub use header::{CONTAINER_VERSION, HEADER_SIZE, Header, MAGIC};

pub const DEFAULT_MAX_OUTPUT_SIZE: u64 = 64 * 1024 * 1024;
pub const DEFAULT_MAX_MEMORY_KIB: u32 = 64 * 1024;

/// Each variant identifies an immutable on-disk algorithm, not an encoder preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Algorithm {
    LzmaV1,
    #[default]
    LzmaV2,
}

impl Algorithm {
    pub const fn id(self) -> u16 {
        match self {
            Self::LzmaV1 | Self::LzmaV2 => 1,
        }
    }

    pub const fn version(self) -> u16 {
        match self {
            Self::LzmaV1 => 1,
            Self::LzmaV2 => 2,
        }
    }

    pub fn from_id_version(id: u16, version: u16) -> Result<Self> {
        match (id, version) {
            (1, 1) => Ok(Self::LzmaV1),
            (1, 2) => Ok(Self::LzmaV2),
            _ => Err(Error::UnsupportedAlgorithm { id, version }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CompressionOptions {
    pub algorithm: Algorithm,
    /// LZMA preset in 0..=9. Presets do not change the algorithm version.
    pub level: u32,
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::default(),
            level: 6,
        }
    }
}

/// Decoder working memory and output have separate limits. These do not include
/// caller-owned input or binding copies, and do not impose a CPU time limit.
#[derive(Clone, Copy, Debug)]
pub struct DecompressionLimits {
    pub max_output_size: u64,
    pub max_memory_kib: u32,
}

impl Default for DecompressionLimits {
    fn default() -> Self {
        Self {
            max_output_size: DEFAULT_MAX_OUTPUT_SIZE,
            max_memory_kib: DEFAULT_MAX_MEMORY_KIB,
        }
    }
}

/// Compress replay bytes using LZMA v2, preset 6.
pub fn compress(replay: &[u8]) -> Result<Vec<u8>> {
    compress_with_options(replay, CompressionOptions::default())
}

/// Compress without normalizing replay fields, timestamps, text or signatures.
pub fn compress_with_options(replay: &[u8], options: CompressionOptions) -> Result<Vec<u8>> {
    if options.level > 9 {
        return Err(Error::InvalidLevel(options.level));
    }
    let payload = match options.algorithm {
        Algorithm::LzmaV1 => lzma::encode(replay, options.level)?,
        Algorithm::LzmaV2 => preprocess::encode(replay, options.level)?,
    };
    let header = Header {
        container_version: CONTAINER_VERSION,
        algorithm_id: options.algorithm.id(),
        algorithm_version: options.algorithm.version(),
        original_size: replay.len() as u64,
        payload_size: payload.len() as u64,
        original_crc32: crc32fast::hash(replay),
        payload_crc32: crc32fast::hash(&payload),
    };
    let mut archive = Vec::new();
    let size = HEADER_SIZE
        .checked_add(payload.len())
        .ok_or(Error::AllocationFailed)?;
    archive
        .try_reserve_exact(size)
        .map_err(|_| Error::AllocationFailed)?;
    archive.extend_from_slice(&header.encode());
    archive.extend_from_slice(&payload);
    Ok(archive)
}

/// Check the complete envelope and payload CRC without decompressing. Unknown
/// algorithm IDs/versions can be inspected; call `Header::algorithm` to resolve.
/// The original CRC and codec stream are validated only during decompression.
pub fn inspect(archive: &[u8]) -> Result<Header> {
    let header = Header::parse(archive)?;
    let payload = &archive[HEADER_SIZE..];
    if payload.len() as u64 != header.payload_size {
        return Err(Error::PayloadSizeMismatch);
    }
    if crc32fast::hash(payload) != header.payload_crc32 {
        return Err(Error::ChecksumMismatch("payload"));
    }
    Ok(header)
}

/// Decode with a 64 MiB output limit and a 64 MiB LZMA working-memory limit.
pub fn decompress(archive: &[u8]) -> Result<Vec<u8>> {
    decompress_with_limits(archive, DecompressionLimits::default())
}

/// Decode exactly one archive. Concatenation, trailing bytes, truncated streams,
/// unsupported versions, size mismatches and damaged checksums are errors.
pub fn decompress_with_limits(archive: &[u8], limits: DecompressionLimits) -> Result<Vec<u8>> {
    let header = inspect(archive)?;
    let algorithm = header.algorithm()?;
    if header.original_size > limits.max_output_size {
        return Err(Error::OutputLimitExceeded {
            size: header.original_size,
            limit: limits.max_output_size,
        });
    }
    let output = match algorithm {
        Algorithm::LzmaV1 => lzma::decode(&archive[HEADER_SIZE..], header.original_size, limits)?,
        Algorithm::LzmaV2 => {
            preprocess::decode(&archive[HEADER_SIZE..], header.original_size, limits)?
        }
    };
    if crc32fast::hash(&output) != header.original_crc32 {
        return Err(Error::ChecksumMismatch("original"));
    }
    Ok(output)
}
