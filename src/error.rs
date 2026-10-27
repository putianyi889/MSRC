use std::io;

/// Errors are reported before returning any decompressed data to the caller.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("truncated MSRC header (expected at least 40 bytes)")]
    TruncatedHeader,
    #[error("invalid MSRC magic bytes")]
    InvalidMagic,
    #[error("unsupported container version: {0}")]
    UnsupportedContainerVersion(u16),
    #[error("unsupported algorithm: id {id}, version {version}")]
    UnsupportedAlgorithm { id: u16, version: u16 },
    #[error("unsupported container flags: {0:#06x}")]
    UnsupportedFlags(u16),
    #[error("{0} CRC-32 mismatch")]
    ChecksumMismatch(&'static str),
    #[error("payload size does not match the container (truncated or trailing data)")]
    PayloadSizeMismatch,
    #[error("compression level must be in 0..=9, got {0}")]
    InvalidLevel(u32),
    #[error("declared output size {size} exceeds limit {limit}")]
    OutputLimitExceeded { size: u64, limit: u64 },
    #[error("decompressed size does not match the declared size")]
    OutputSizeMismatch,
    #[error("invalid LZMA v1 framing: {0}")]
    InvalidLzmaFraming(&'static str),
    #[error("allocation failed or size is not representable on this platform")]
    AllocationFailed,
    #[error("LZMA codec error: {0}")]
    Codec(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
