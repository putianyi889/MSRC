//! Bounded framing readers, based on the pinned upstream sources documented in
//! docs/research/mouse-events/README.md. No replay simulation or normalization.

use crate::Result;

use super::{Kind, Region, invalid};

pub(super) struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    pub fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(count)
            .ok_or_else(|| invalid("field size overflow"))?;
        let value = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| invalid("truncated replay field"))?;
        self.pos = end;
        Ok(value)
    }
    pub fn uint(&mut self, width: usize) -> Result<u32> {
        Ok(self
            .take(width)?
            .iter()
            .fold(0, |n, &b| (n << 8) | b as u32))
    }
    pub fn cstring(&mut self) -> Result<()> {
        let count = self
            .data
            .get(self.pos..)
            .and_then(|b| b.iter().position(|&v| v == 0))
            .ok_or_else(|| invalid("unterminated replay string"))?;
        self.take(count + 1)?;
        Ok(())
    }
}

pub(super) fn find(data: &[u8]) -> Result<Vec<Region>> {
    if data.starts_with(b"*rmv") {
        return Ok(vec![rmv(data)?]);
    }
    if data.starts_with(b"\x11M") {
        return Ok(vec![mvf(data)?]);
    }
    match data.first() {
        Some(3 | 4) => Ok(vec![evf(data)?]),
        Some(0) => {
            let mut r = Reader::new(data);
            r.take(1)?;
            let checksum = r.uint(2)? as usize;
            let mut regions = Vec::new();
            while r.pos < data.len() {
                r.cstring()?;
                let size = r.uint(4)? as usize;
                let base = r.pos;
                let mut region = evf(r.take(size)?)?;
                region.offset = region
                    .offset
                    .checked_add(base)
                    .ok_or_else(|| invalid("offset overflow"))?;
                if region.records != 0 {
                    regions.push(region);
                }
                r.take(checksum)?;
            }
            Ok(regions)
        }
        Some(35..=52) => Ok(vec![avf(data)?]),
        _ => Err(invalid("unrecognized replay format")),
    }
}

fn avf(data: &[u8]) -> Result<Region> {
    let mut r = Reader::new(data);
    r.take(5)?;
    let mines = match r.uint(1)? {
        3 => 10,
        4 => 40,
        5 => 99,
        6 => {
            r.take(2)?;
            r.uint(2)? as usize
        }
        _ => return Err(invalid("unknown AVF level")),
    };
    r.take(2 * mines)?;
    // Upstream AVF synchronization heuristic. Validate all subsequent records.
    let text = &data[r.pos..];
    let start = text
        .windows(3)
        .position(|b| b[0] == b'[' && b[1] >= b'0' && b[1] <= b'3' && b[2] == b'|')
        .ok_or_else(|| invalid("missing AVF time header"))?;
    let end = text[start..]
        .iter()
        .position(|&b| b == b']')
        .ok_or_else(|| invalid("truncated AVF time header"))?
        + start;
    let header = &text[start..end];
    if !header.contains(&b'B') || !header.contains(&b'T') {
        return Err(invalid("invalid AVF time header"));
    }
    r.take(end + 1)?;
    loop {
        if r.take(1)?[0] == 1 && r.pos >= 3 && data[r.pos - 2] <= 1 {
            r.pos -= 3;
            break;
        }
    }
    let offset = r.pos;
    let mut records = 0;
    loop {
        let e = r.take(8)?;
        if e[2] == 0 && e[6] == 0 {
            break;
        }
        if !matches!(e[0], 1 | 3 | 5 | 9 | 11 | 17 | 21 | 33 | 65 | 145 | 193) || e[4] >= 100 {
            return Err(invalid("unsupported AVF event"));
        }
        records += 1;
    }
    Ok(Region {
        kind: Kind::Avf,
        offset,
        length: records * 8,
        records,
        permutation: Vec::new(),
    })
}

fn rmv(data: &[u8]) -> Result<Region> {
    let mut r = Reader::new(data);
    r.take(4)?;
    let kind = match r.uint(2)? {
        1 => Kind::Rmv1,
        2 => {
            r.take(2)?;
            Kind::Rmv2
        }
        _ => return Err(invalid("unknown RMV version")),
    };
    r.take(4)?;
    let mut metadata = 0usize;
    for _ in 0..6 {
        metadata += r.uint(2)? as usize;
    }
    let video_size = r.uint(4)? as usize;
    let checksum_size = r.uint(2)? as usize;
    r.take(metadata)?;
    let offset = r.pos;
    let video = r.take(video_size)?;
    r.take(checksum_size)?;
    if r.pos != data.len() {
        return Err(invalid("unexpected RMV trailing data"));
    }
    let mut events = Reader::new(video);
    let mut records = 0;
    let mut mouse_seen = false;
    loop {
        let start = events.pos;
        let code = events.uint(1)? as u8;
        if (15..=17).contains(&code) {
            events.take(3)?;
            return Ok(Region {
                kind,
                offset,
                length: start,
                records,
                permutation: Vec::new(),
            });
        }
        if code == 28 && !mouse_seen {
            return Err(invalid("RMV reduced move before initial position"));
        }
        let size = kind.record_size(code)?;
        events.take(size - 1)?;
        mouse_seen |= (1..=7).contains(&code);
        records += 1;
    }
}

fn evf(data: &[u8]) -> Result<Region> {
    let mut r = Reader::new(data);
    let kind = match r.uint(1)? {
        3 => Kind::Evf3,
        4 => Kind::Evf4,
        _ => return Err(invalid("unknown EVF version")),
    };
    let flags = r.uint(1)?;
    r.take(1)?;
    let rows = r.uint(1)? as usize;
    let cols = r.uint(1)? as usize;
    r.take(7)?;
    if kind == Kind::Evf3 {
        r.take(3)?;
        for _ in 0..8 {
            r.cstring()?;
        }
    } else {
        r.take(22)?;
        r.cstring()?;
        if flags & 8 != 0 {
            r.cstring()?;
            r.cstring()?;
        }
        for _ in 0..3 {
            r.cstring()?;
        }
        let size = r.uint(2)? as usize;
        r.take(size)?;
    }
    r.take((rows * cols).div_ceil(8))?;
    if kind == Kind::Evf4 {
        for _ in 0..r.uint(2)? {
            r.cstring()?;
        }
    }
    let offset = r.pos;
    let mut records = 0;
    loop {
        let start = r.pos;
        let code = r.uint(1)? as u8;
        if code == 0 || (kind == Kind::Evf3 && code == 255) {
            let checksum = if kind == Kind::Evf4 {
                r.uint(2)? as usize
            } else if code == 0 {
                32
            } else {
                0
            };
            r.take(checksum)?;
            if r.pos != data.len() {
                return Err(invalid("unexpected EVF trailing data"));
            }
            return Ok(Region {
                kind,
                offset,
                length: start - offset,
                records,
                permutation: Vec::new(),
            });
        }
        r.take(kind.record_size(code)? - 1)?;
        records += 1;
    }
}

fn mvf(data: &[u8]) -> Result<Region> {
    let old = match data.get(27) {
        Some(b'5') => true,
        Some(b'6' | b'7') => false,
        _ => return Err(invalid("unknown MVF version")),
    };
    let mut r = Reader::new(data);
    r.take(if old { 74 } else { 71 })?;
    r.take(12 + if old { 11 } else { 1 })?;
    r.take(2)?;
    let mines = r.uint(2)? as usize;
    r.take(2 * mines)?;
    let size = r.uint(1)? as usize;
    r.take(size)?;
    let seed = r.uint(2)? as f64;
    let (n1, n2) = (seed.sqrt(), (seed + 1000.0).sqrt());
    let (n3, n4) = ((n1 + 1000.0).sqrt(), (n2 + 1000.0).sqrt());
    let values = [
        (n3 + 1000.0).cos(),
        n2.sqrt().sin(),
        n3.cos(),
        (n1.sqrt() + 1000.0).sin(),
        n4.cos(),
        n4.sin(),
    ];
    let mut digits = String::new();
    for value in &values[..if old { 5 } else { 6 }] {
        digits.push_str(&format!(
            "{:08}",
            (value.abs() * 100000000.0).round() as u64
        ));
    }
    let width = if old { 5 } else { 6 };
    if digits.len() != width * 8 {
        return Err(invalid("invalid MVF permutation"));
    }
    let mut permutation: Vec<u8> = (0..(width * 8) as u8).collect();
    permutation.sort_by_key(|&bit| (digits.as_bytes()[bit as usize], bit));
    let records = r.uint(3)? as usize;
    let offset = r.pos;
    let length = records
        .checked_mul(width)
        .ok_or_else(|| invalid("MVF event size overflow"))?;
    let events = r.take(length)?;
    if old {
        for record in events.chunks_exact(5) {
            let logical = super::mvf_logical(record, &permutation);
            if (logical >> 21) & 127 >= 100 {
                return Err(invalid("noncanonical MVF hundredths"));
            }
        }
    }
    Ok(Region {
        kind: if old { Kind::Mvf097 } else { Kind::Mvf2007 },
        offset,
        length,
        records,
        permutation,
    })
}
