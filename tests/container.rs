use msrc::{
    Algorithm, CompressionOptions, DecompressionLimits, Error, HEADER_SIZE, compress,
    compress_with_options, decompress, decompress_with_limits, inspect,
};

fn fix_header(archive: &mut [u8]) {
    let crc = crc32fast::hash(&archive[..36]);
    archive[36..40].copy_from_slice(&crc.to_le_bytes());
}

fn fix_payload(archive: &mut [u8]) {
    let size = (archive.len() - HEADER_SIZE) as u64;
    archive[20..28].copy_from_slice(&size.to_le_bytes());
    let crc = crc32fast::hash(&archive[HEADER_SIZE..]);
    archive[32..36].copy_from_slice(&crc.to_le_bytes());
    fix_header(archive);
}

fn fixture() -> Vec<u8> {
    include_str!("fixtures/lzma-v1.hex")
        .split_whitespace()
        .map(|hex| u8::from_str_radix(hex, 16).unwrap())
        .collect()
}

fn fixture_original() -> Vec<u8> {
    let mut bytes = b"MSRC interoperability fixture\n".to_vec();
    bytes.extend((0..=255).cycle().take(1024));
    bytes
}

fn compress_v1(input: &[u8]) -> msrc::Result<Vec<u8>> {
    compress_with_options(
        input,
        CompressionOptions {
            algorithm: Algorithm::LzmaV1,
            level: 6,
        },
    )
}

#[test]
fn byte_exact_roundtrips() {
    let samples = [
        Vec::new(),
        vec![0],
        vec![255],
        (0..=255).collect(),
        vec![0; 65536],
        fixture_original(),
        // Opaque, intentionally not parsed or claimed to be valid replays.
        b"RAWVF\r\n0.001 mv 12 24\r\nunknown metadata\0\xff".to_vec(),
    ];
    for input in samples {
        for algorithm in [Algorithm::LzmaV1, Algorithm::LzmaV2] {
            let archive = compress_with_options(
                &input,
                CompressionOptions {
                    algorithm,
                    level: 6,
                },
            )
            .unwrap();
            assert_eq!(decompress(&archive).unwrap(), input);
            let info = inspect(&archive).unwrap();
            assert_eq!(info.original_size, input.len() as u64);
            assert_eq!(info.payload_size, (archive.len() - HEADER_SIZE) as u64);
            assert_eq!(info.algorithm().unwrap(), algorithm);
            assert_eq!(info.container_version, 1);
        }
    }
}

#[test]
fn incompressible_bytes_roundtrip() {
    let mut state = 0xdead_beef_u32;
    let input: Vec<u8> = (0..16384)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    assert_eq!(decompress(&compress(&input).unwrap()).unwrap(), input);
}

#[test]
fn repetitive_bytes_shrink() {
    let original = b"0.001 mv 12 24\n0.002 lc 12 24\n".repeat(1000);
    assert!(compress(&original).unwrap().len() < original.len() / 10);
}

#[test]
fn presets_are_compatible() {
    for algorithm in [Algorithm::LzmaV1, Algorithm::LzmaV2] {
        for level in 0..=9 {
            let options = CompressionOptions { level, algorithm };
            let archive = compress_with_options(b"preset compatibility", options).unwrap();
            assert_eq!(decompress(&archive).unwrap(), b"preset compatibility");
            assert_eq!(
                inspect(&archive).unwrap().algorithm_version,
                algorithm.version()
            );
        }
    }
    assert!(matches!(
        compress_with_options(
            b"",
            CompressionOptions {
                level: 10,
                ..Default::default()
            }
        ),
        Err(Error::InvalidLevel(10))
    ));
}

#[test]
fn independent_python_lzma_fixture_decodes() {
    assert_eq!(decompress(&fixture()).unwrap(), fixture_original());
}

#[test]
fn every_truncation_and_bit_flip_fails() {
    let archive = compress(b"a small replay with a few events").unwrap();
    for end in 0..archive.len() {
        assert!(decompress(&archive[..end]).is_err(), "truncated at {end}");
    }
    for index in 0..archive.len() {
        let mut damaged = archive.clone();
        damaged[index] ^= 1;
        assert!(decompress(&damaged).is_err(), "damaged at {index}");
    }
}

#[test]
fn truncated_codec_streams_fail_even_with_valid_envelope_checksums() {
    let archive = compress(b"LZMA end marker validation").unwrap();
    for end in HEADER_SIZE..archive.len() {
        let mut truncated = archive[..end].to_vec();
        fix_payload(&mut truncated);
        assert!(decompress(&truncated).is_err(), "truncated at {end}");
    }
}

#[test]
fn unsupported_versions_and_flags_are_explicit() {
    let original = compress_v1(b"future").unwrap();
    for (offset, value) in [(4, 2_u16), (6, 42), (8, 3), (10, 1)] {
        let mut archive = original.clone();
        archive[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        fix_header(&mut archive);
        let error = decompress(&archive).unwrap_err();
        match offset {
            4 => assert!(matches!(error, Error::UnsupportedContainerVersion(2))),
            6 => assert!(matches!(
                error,
                Error::UnsupportedAlgorithm { id: 42, version: 1 }
            )),
            8 => assert!(matches!(
                error,
                Error::UnsupportedAlgorithm { id: 1, version: 3 }
            )),
            10 => assert!(matches!(error, Error::UnsupportedFlags(1))),
            _ => unreachable!(),
        }
        if offset == 6 || offset == 8 {
            assert!(inspect(&archive).is_ok());
        }
    }
}

#[test]
fn trailing_data_and_concatenated_archives_fail() {
    let archive = compress(b"single member").unwrap();
    let mut trailing = archive.clone();
    trailing.push(0);
    assert!(matches!(
        decompress(&trailing),
        Err(Error::PayloadSizeMismatch)
    ));
    fix_payload(&mut trailing);
    assert!(matches!(
        decompress(&trailing),
        Err(Error::InvalidLzmaFraming(_))
    ));
    let mut concatenated = archive.clone();
    concatenated.extend_from_slice(&archive);
    assert!(decompress(&concatenated).is_err());
    let mut codec_concatenated = archive.clone();
    codec_concatenated.extend_from_slice(&archive[HEADER_SIZE..]);
    fix_payload(&mut codec_concatenated);
    assert!(decompress(&codec_concatenated).is_err());
}

#[test]
fn false_original_sizes_and_checksum_fail() {
    let archive = compress(b"12345678").unwrap();
    for size in [0_u64, 7, 9, u64::MAX] {
        let mut changed = archive.clone();
        changed[12..20].copy_from_slice(&size.to_le_bytes());
        fix_header(&mut changed);
        assert!(decompress(&changed).is_err());
    }
    let mut changed = archive;
    changed[28] ^= 1;
    fix_header(&mut changed);
    assert!(matches!(
        decompress(&changed),
        Err(Error::ChecksumMismatch("original"))
    ));
}

#[test]
fn output_and_memory_limits_are_enforced() {
    let archive = compress(&[0; 4096]).unwrap();
    let mut limits = DecompressionLimits {
        max_output_size: 4095,
        ..Default::default()
    };
    assert!(matches!(
        decompress_with_limits(&archive, limits),
        Err(Error::OutputLimitExceeded { .. })
    ));
    limits.max_output_size = 4096;
    assert_eq!(
        decompress_with_limits(&archive, limits).unwrap().len(),
        4096
    );
    limits.max_memory_kib = 0;
    assert!(decompress_with_limits(&archive, limits).is_err());
    let mut oversized_dictionary = archive;
    oversized_dictionary[HEADER_SIZE + 1..HEADER_SIZE + 5]
        .copy_from_slice(&(1_u32 << 30).to_le_bytes());
    fix_payload(&mut oversized_dictionary);
    assert!(decompress(&oversized_dictionary).is_err());
}

#[test]
fn empty_output_accepts_zero_output_budget() {
    let archive = compress(b"").unwrap();
    let limits = DecompressionLimits {
        max_output_size: 0,
        ..Default::default()
    };
    assert!(decompress_with_limits(&archive, limits).unwrap().is_empty());
}

#[test]
fn invalid_inner_header_is_rejected() {
    let original = compress_v1(b"inner framing").unwrap();
    let mut properties = original.clone();
    properties[HEADER_SIZE] = 255;
    fix_payload(&mut properties);
    assert!(decompress(&properties).is_err());
    let mut known_size = original;
    known_size[HEADER_SIZE + 5..HEADER_SIZE + 13].copy_from_slice(&13_u64.to_le_bytes());
    fix_payload(&mut known_size);
    assert!(matches!(
        decompress(&known_size),
        Err(Error::InvalidLzmaFraming(_))
    ));
}
