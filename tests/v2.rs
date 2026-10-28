use msrc::{
    Algorithm, CompressionOptions, DecompressionLimits, Error, HEADER_SIZE, compress,
    compress_with_options, decompress, decompress_with_limits, inspect,
};

fn hex(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}

fn fixtures() -> [(Vec<u8>, Vec<u8>); 2] {
    [
        (
            hex(include_str!("fixtures/lzma-v2-evf3.hex")),
            hex(include_str!("fixtures/lzma-v2-evf3.original.hex")),
        ),
        (
            hex(include_str!("fixtures/lzma-v2-evf4.hex")),
            hex(include_str!("fixtures/lzma-v2-evf4.original.hex")),
        ),
    ]
}

fn fix_envelope(archive: &mut [u8]) {
    let size = (archive.len() - HEADER_SIZE) as u64;
    archive[20..28].copy_from_slice(&size.to_le_bytes());
    let payload_crc = crc32fast::hash(&archive[HEADER_SIZE..]);
    archive[32..36].copy_from_slice(&payload_crc.to_le_bytes());
    let header_crc = crc32fast::hash(&archive[..36]);
    archive[36..40].copy_from_slice(&header_crc.to_le_bytes());
}

#[test]
fn independent_filtered_fixtures_and_both_encoders_remain_usable() {
    for (archive, original) in fixtures() {
        assert_eq!(
            inspect(&archive).unwrap().algorithm().unwrap(),
            Algorithm::LzmaV2
        );
        assert_eq!(decompress(&archive).unwrap(), original);
        for algorithm in [Algorithm::LzmaV1, Algorithm::LzmaV2] {
            let encoded = compress_with_options(
                &original,
                CompressionOptions {
                    algorithm,
                    level: 6,
                },
            )
            .unwrap();
            assert_eq!(inspect(&encoded).unwrap().algorithm().unwrap(), algorithm);
            assert_eq!(decompress(&encoded).unwrap(), original);
        }
        let encoded = compress(&original).unwrap();
        assert_eq!(encoded[HEADER_SIZE], 1); // exercise the filtered default, not only raw fallback
    }
}

#[test]
fn filtered_stream_truncations_and_forged_descriptors_are_rejected() {
    for (archive, original) in fixtures() {
        for end in HEADER_SIZE..archive.len() {
            let mut truncated = archive[..end].to_vec();
            fix_envelope(&mut truncated);
            assert!(
                decompress(&truncated).is_err(),
                "accepted truncation at {end}"
            );
        }
        let mut changed = archive.clone();
        changed[HEADER_SIZE + 6..HEADER_SIZE + 14].copy_from_slice(&u64::MAX.to_le_bytes());
        fix_envelope(&mut changed);
        assert!(matches!(
            decompress(&changed),
            Err(Error::InvalidPreprocessing(_))
        ));
        let limits = DecompressionLimits {
            max_output_size: original.len() as u64 - 1,
            ..Default::default()
        };
        assert!(matches!(
            decompress_with_limits(&archive, limits),
            Err(Error::OutputLimitExceeded { .. })
        ));
        let limits = DecompressionLimits {
            max_memory_kib: 0,
            ..Default::default()
        };
        assert!(decompress_with_limits(&archive, limits).is_err());
    }
}
