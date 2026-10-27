use crate::{Algorithm, Error, Result};

pub const MAGIC: [u8; 4] = *b"MSRC";
pub const CONTAINER_VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 40;

/// Metadata read from a complete archive. Checksums detect damage, not tampering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub container_version: u16,
    pub algorithm_id: u16,
    pub algorithm_version: u16,
    pub original_size: u64,
    pub payload_size: u64,
    pub original_crc32: u32,
    pub payload_crc32: u32,
}

impl Header {
    /// Resolve the immutable algorithm ID/version pair to a supported decoder.
    pub fn algorithm(&self) -> Result<Algorithm> {
        Algorithm::from_id_version(self.algorithm_id, self.algorithm_version)
    }

    pub(crate) fn encode(self) -> [u8; HEADER_SIZE] {
        let mut bytes = [0; HEADER_SIZE];
        bytes[..4].copy_from_slice(&MAGIC);
        bytes[4..6].copy_from_slice(&self.container_version.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.algorithm_id.to_le_bytes());
        bytes[8..10].copy_from_slice(&self.algorithm_version.to_le_bytes());
        // 10..12: reserved flags, zero in container v1.
        bytes[12..20].copy_from_slice(&self.original_size.to_le_bytes());
        bytes[20..28].copy_from_slice(&self.payload_size.to_le_bytes());
        bytes[28..32].copy_from_slice(&self.original_crc32.to_le_bytes());
        bytes[32..36].copy_from_slice(&self.payload_crc32.to_le_bytes());
        let checksum = crc32fast::hash(&bytes[..36]);
        bytes[36..40].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        let bytes: &[u8; HEADER_SIZE] = bytes
            .get(..HEADER_SIZE)
            .ok_or(Error::TruncatedHeader)?
            .try_into()
            .map_err(|_| Error::TruncatedHeader)?;
        if bytes[..4] != MAGIC {
            return Err(Error::InvalidMagic);
        }
        let u16_at = |i| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let u32_at = |i| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        let u64_at = |i| {
            u64::from_le_bytes([
                bytes[i],
                bytes[i + 1],
                bytes[i + 2],
                bytes[i + 3],
                bytes[i + 4],
                bytes[i + 5],
                bytes[i + 6],
                bytes[i + 7],
            ])
        };
        if crc32fast::hash(&bytes[..36]) != u32_at(36) {
            return Err(Error::ChecksumMismatch("header"));
        }
        let container_version = u16_at(4);
        if container_version != CONTAINER_VERSION {
            return Err(Error::UnsupportedContainerVersion(container_version));
        }
        let flags = u16_at(10);
        if flags != 0 {
            return Err(Error::UnsupportedFlags(flags));
        }
        Ok(Self {
            container_version,
            algorithm_id: u16_at(6),
            algorithm_version: u16_at(8),
            original_size: u64_at(12),
            payload_size: u64_at(20),
            original_crc32: u32_at(28),
            payload_crc32: u32_at(32),
        })
    }
}
