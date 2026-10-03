//! Font metadata via ttf-parser (TTF/OTF/TTC; WOFF1 is unwrapped first).

use crate::util::{OrStr, Res};
use serde::Serialize;
use std::io::Read;
use std::path::Path;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Axis {
    pub tag: String,
    pub name: String,
    pub min: f32,
    pub default: f32,
    pub max: f32,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FontInfo {
    pub family: Option<String>,
    pub subfamily: Option<String>,
    pub full_name: Option<String>,
    pub version: Option<String>,
    pub designer: Option<String>,
    pub manufacturer: Option<String>,
    pub copyright: Option<String>,
    pub license: Option<String>,
    pub glyphs: u16,
    pub units_per_em: u16,
    pub faces: u32,
    pub monospaced: bool,
    pub variable: bool,
    pub axes: Vec<Axis>,
    /// Sample of mapped code points for the glyph grid.
    pub codepoints: Vec<u32>,
    /// Total number of mapped code points.
    pub coverage: usize,
    pub format: String,
}

pub fn info(path: &Path, ext: &str) -> Res<FontInfo> {
    let mut data = Vec::new();
    std::fs::File::open(path)
        .or_str()?
        .take(64 << 20)
        .read_to_end(&mut data)
        .or_str()?;
    if ext == "woff2" || data.starts_with(b"wOF2") {
        // WOFF2 needs Brotli + table transforms; the webview renders it, we just label it.
        return Ok(FontInfo { format: "WOFF2".into(), faces: 1, ..Default::default() });
    }
    let (data, format) = if data.starts_with(b"wOFF") {
        (unwrap_woff(&data)?, "WOFF")
    } else if data.starts_with(b"OTTO") {
        (data, "OpenType (CFF)")
    } else if data.starts_with(b"ttcf") {
        (data, "TrueType Collection")
    } else {
        (data, "TrueType")
    };
    let faces = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
    let face = ttf_parser::Face::parse(&data, 0).ctx("Not a valid font")?;
    let name = |id: u16| -> Option<String> {
        face.names()
            .into_iter()
            .filter(|n| n.name_id == id)
            .find_map(|n| n.to_string())
            .filter(|s| !s.trim().is_empty())
    };
    use ttf_parser::name_id;
    let mut fi = FontInfo {
        family: name(name_id::TYPOGRAPHIC_FAMILY).or_else(|| name(name_id::FAMILY)),
        subfamily: name(name_id::TYPOGRAPHIC_SUBFAMILY).or_else(|| name(name_id::SUBFAMILY)),
        full_name: name(name_id::FULL_NAME),
        version: name(name_id::VERSION),
        designer: name(name_id::DESIGNER),
        manufacturer: name(name_id::MANUFACTURER),
        copyright: name(name_id::COPYRIGHT_NOTICE),
        license: name(name_id::LICENSE),
        glyphs: face.number_of_glyphs(),
        units_per_em: face.units_per_em(),
        faces,
        monospaced: face.is_monospaced(),
        variable: face.is_variable(),
        format: format.into(),
        ..Default::default()
    };
    for a in face.variation_axes() {
        fi.axes.push(Axis {
            tag: a.tag.to_string(),
            name: name(a.name_id).unwrap_or_else(|| a.tag.to_string()),
            min: a.min_value,
            default: a.def_value,
            max: a.max_value,
        });
    }
    let mut cps: Vec<u32> = Vec::new();
    if let Some(cmap) = face.tables().cmap {
        for st in cmap.subtables {
            if st.is_unicode() {
                st.codepoints(|c| cps.push(c));
            }
        }
    }
    cps.sort_unstable();
    cps.dedup();
    cps.retain(|&c| c >= 0x20 && !(0x7F..0xA0).contains(&c) && !(0xD800..0xE000).contains(&c));
    fi.coverage = cps.len();
    cps.truncate(1024);
    fi.codepoints = cps;
    Ok(fi)
}

/// Convert WOFF 1.0 into a plain sfnt (zlib-compressed tables).
fn unwrap_woff(d: &[u8]) -> Res<Vec<u8>> {
    let be16 = |o: usize| -> Res<u16> {
        d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or_else(|| "truncated WOFF".to_string())
    };
    let be32 = |o: usize| -> Res<u32> {
        d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| "truncated WOFF".to_string())
    };
    let flavor = be32(4)?;
    let num = be16(12)? as usize;
    let mut tables = Vec::with_capacity(num);
    for i in 0..num {
        let o = 44 + i * 20;
        let (tag, off, clen, olen) = (be32(o)?, be32(o + 4)? as usize, be32(o + 8)? as usize, be32(o + 12)? as usize);
        let raw = d.get(off..off + clen).ok_or("truncated WOFF table")?;
        let data = if clen < olen {
            let mut v = Vec::with_capacity(olen);
            flate2::read::ZlibDecoder::new(raw).take(olen as u64).read_to_end(&mut v).or_str()?;
            v
        } else {
            raw.to_vec()
        };
        tables.push((tag, data));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&(num as u16).to_be_bytes());
    let mut pow = 1u16;
    let mut log = 0u16;
    while (pow as usize) * 2 <= num {
        pow *= 2;
        log += 1;
    }
    out.extend_from_slice(&(pow * 16).to_be_bytes());
    out.extend_from_slice(&log.to_be_bytes());
    out.extend_from_slice(&((num as u16).saturating_mul(16).saturating_sub(pow * 16)).to_be_bytes());
    let mut offset = 12 + num * 16;
    let mut dir = Vec::new();
    let mut body = Vec::new();
    for (tag, data) in &tables {
        dir.extend_from_slice(&tag.to_be_bytes());
        dir.extend_from_slice(&0u32.to_be_bytes());
        dir.extend_from_slice(&(offset as u32).to_be_bytes());
        dir.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + num * 16 + body.len();
    }
    out.extend(dir);
    out.extend(body);
    Ok(out)
}
