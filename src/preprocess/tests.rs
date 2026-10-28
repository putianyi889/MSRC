use super::*;

fn region(kind: Kind, length: usize, records: usize) -> Region {
    Region {
        kind,
        offset: 0,
        length,
        records,
        permutation: Vec::new(),
    }
}

fn roundtrip(kind: Kind, bytes: &[u8], count: usize) -> Vec<u8> {
    let r = region(kind, bytes.len(), count);
    let encoded = transform(&r, bytes, false).unwrap();
    assert_eq!(encoded.len(), bytes.len());
    assert_eq!(transform(&r, &encoded, true).unwrap(), bytes);
    encoded
}

#[test]
fn avf_golden_columns_and_hundredth_rollover() {
    let original = [145, 0, 1, 12, 99, 0, 0, 9, 5, 0, 2, 10, 0, 0, 0, 9];
    let shuffled = roundtrip(Kind::Avf, &original, 2);
    assert_eq!(
        shuffled,
        [145, 5, 199, 1, 0, 0, 0, 0, 24, 3, 0, 0, 18, 0, 0, 0]
    );
}

#[test]
fn evf_absolute_time_wrap_and_full_coordinate_domain() {
    let original = [
        2, 255, 255, 255, 128, 0, 255, 255, 3, 0, 0, 0, 127, 255, 0, 0,
    ];
    assert_eq!(
        roundtrip(Kind::Evf3, &original, 2),
        [2, 3, 255, 1, 255, 0, 255, 0, 255, 1, 255, 0, 1, 2, 0, 0]
    );
}

#[test]
fn existing_evf_deltas_receive_zigzag_and_keep_pause_and_state() {
    let original = [2, 7, 0, 10, 0, 20, 255, 3, 232, 81, 2, 3, 5, 255, 254, 0, 3];
    let encoded = roundtrip(Kind::Evf4, &original, 4);
    assert_eq!(
        encoded,
        [2, 255, 81, 3, 7, 2, 5, 20, 3, 0, 0, 40, 6, 0, 0, 232, 3]
    );
}

#[test]
fn rmv_reduced_moves_update_the_next_absolute_baseline() {
    let original = [
        1, 255, 255, 254, 255, 255, 254, 0, 2, 9, 255, 0, 28, 3, 248, 3, 0, 0, 2, 127, 255, 252,
        255, 249,
    ];
    let encoded = roundtrip(Kind::Rmv2, &original, 4);
    assert_eq!(&encoded[..4], &[1, 9, 28, 3]);
    // The second normal event is +1 ms, -1 px, -1 px relative to the
    // position/time reached by the reduced move, not the earlier normal event.
    assert_eq!(&encoded[4..10], &[254, 1, 255, 0, 255, 0]);
    assert_eq!(&encoded[12..16], &[3, 1, 0, 0]);
    assert_eq!(&encoded[16..20], &[4, 1, 0, 0]);
    assert_eq!(&encoded[22..], &[3, 31]); // -1 -> 1, -8 -> 15
}

#[test]
fn rmv_timestamp_changes_keep_all_32_bits_and_flags() {
    let original = [
        0, 255, 255, 255, 255, 0, 0, 0, 0, 0, 3, 0, 0, 0, 255, 255, 255, 128, 0,
    ];
    roundtrip(Kind::Rmv1, &original, 3);
}

#[test]
fn zigzag_and_modular_difference_are_bijections_at_each_width() {
    for bits in [4, 8, 9, 11, 16, 17, 22, 24, 32] {
        let limit = mask(bits);
        for v in [0, 1, 2, limit, limit - 1, limit / 2, limit / 2 + 1] {
            assert_eq!(unzigzag(zigzag(v, bits), bits), v);
        }
        let (mut forward, mut backward) = (0, 0);
        for i in 0u32..=65535 {
            let v = i.wrapping_mul(0x9e3779b9) & limit;
            let encoded = delta(v, &mut forward, bits, true, false);
            assert_eq!(delta(encoded, &mut backward, bits, true, true), v);
        }
    }
}

fn evf3(events: &[u8]) -> Vec<u8> {
    let mut data = vec![3, 192, 0, 2, 2, 0, 1, 16, 0, 0, 0, 1, 0, 0, 0];
    data.extend_from_slice(&[0; 8]);
    data.push(128);
    data.extend_from_slice(events);
    data.push(255);
    data
}

#[test]
fn evfs_regions_restart_baselines_and_preserve_checksums_and_names() {
    let mut original = vec![0, 0, 2];
    for (name, events) in [
        (b"a\0", [2, 0, 0, 0, 0, 16, 0, 32]),
        (b"b\0", [2, 0, 0, 1, 0, 48, 0, 64]),
    ] {
        let bytes = evf3(&events);
        original.extend_from_slice(name);
        original.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        original.extend_from_slice(&bytes);
        original.extend_from_slice(b"cs");
    }
    let regions = parse::find(&original).unwrap();
    assert_eq!(regions.len(), 2);
    let mut restored = original.clone();
    for r in &regions {
        let range = r.offset..r.offset + r.length;
        let transformed = transform(r, &original[range.clone()], false).unwrap();
        restored[range].copy_from_slice(&transform(r, &transformed, true).unwrap());
    }
    assert_eq!(restored, original);
}

#[test]
fn mvf_residual_bits_and_both_packed_layouts_roundtrip() {
    for (old, record) in [
        (true, vec![0x66, 0x38, 0x84, 0x22, 0x7d]),
        (false, vec![0xec, 0x28, 0x20, 0xad, 0x02, 0x15]),
    ] {
        let mut original = vec![0; if old { 74 } else { 71 }];
        original[..2].copy_from_slice(b"\x11M");
        original[27] = if old { b'5' } else { b'7' };
        original.extend_from_slice(&vec![0; 12 + if old { 11 } else { 1 }]);
        original.extend_from_slice(&[2, 2, 0, 0, 0, 0x30, 0x39, 0, 0, 1]);
        original.extend_from_slice(&record);
        original.extend_from_slice(b"opaque trailer");
        let mut regions = parse::find(&original).unwrap();
        let r = regions.pop().unwrap();
        let input = &original[r.offset..r.offset + r.length];
        let encoded = transform(&r, input, false).unwrap();
        assert_eq!(encoded.len(), record.len());
        assert_eq!(transform(&r, &encoded, true).unwrap(), record);
        let logical = mvf_logical(&record, &r.permutation);
        assert_ne!(logical >> if old { 38 } else { 47 }, 0);
    }
}

#[test]
fn invalid_descriptors_and_unknown_modes_are_errors() {
    let limits = DecompressionLimits::default();
    for payload in [
        vec![],
        vec![2],
        vec![1, 0, 0, 0, 0],
        vec![1, 255, 255, 255, 255],
    ] {
        assert!(decode(&payload, 16, limits).is_err());
    }
    let mut payload = vec![1, 1, 0, 0, 0, Kind::Evf3 as u8];
    payload.extend_from_slice(&0u64.to_le_bytes());
    payload.extend_from_slice(&8u64.to_le_bytes());
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&lzma::encode(&[99, 0, 0, 0, 0, 0, 0, 0], 0).unwrap());
    assert!(decode(&payload, 8, limits).is_err()); // unknown type, valid LZMA
    payload[6..14].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(decode(&payload, 8, limits).is_err()); // overflow/out of bounds
}

#[test]
fn format_like_arbitrary_bytes_fall_back_without_panics() {
    let mut state = 0x12345678u32;
    for len in 0..160 {
        let mut bytes: Vec<u8> = (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        for prefix in [
            b"*rmv".as_slice(),
            b"\x11M",
            b"\x03",
            b"\x04",
            b"\x00",
            b"\x34",
        ] {
            if len >= prefix.len() {
                bytes[..prefix.len()].copy_from_slice(prefix);
            }
            let payload = encode(&bytes, 0).unwrap();
            assert_eq!(
                decode(&payload, len as u64, DecompressionLimits::default()).unwrap(),
                bytes
            );
        }
    }
}
