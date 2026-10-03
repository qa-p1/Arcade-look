//! Image metadata (dimensions, EXIF) and decoding of formats webviews can't display:
//! TIFF/TGA/DDS/HDR/EXR/QOI/PNM (via `image`), PSD composites, and camera RAW previews.

use crate::util::{OrStr, Res};
use serde::Serialize;
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub exif: Vec<(String, String)>,
}

pub fn info(path: &Path, kind_format: &str) -> Res<ImageInfo> {
    let mut ii = ImageInfo::default();
    if kind_format == "psd" || kind_format == "psb" {
        if let Ok(h) = psd_header(path) {
            ii.width = Some(h.width);
            ii.height = Some(h.height);
        }
    } else if let Ok(s) = imagesize::size(path) {
        ii.width = Some(s.width as u32);
        ii.height = Some(s.height as u32);
    }
    ii.exif = exif_fields(path);
    Ok(ii)
}

fn exif_fields(path: &Path) -> Vec<(String, String)> {
    let Ok(f) = std::fs::File::open(path) else { return Vec::new() };
    let Ok(ex) = exif::Reader::new().read_from_container(&mut BufReader::new(f)) else {
        return Vec::new();
    };
    use exif::{In, Tag};
    let wanted: &[(Tag, &str)] = &[
        (Tag::Make, "Camera make"),
        (Tag::Model, "Camera model"),
        (Tag::LensModel, "Lens"),
        (Tag::DateTimeOriginal, "Taken"),
        (Tag::ExposureTime, "Exposure"),
        (Tag::FNumber, "Aperture"),
        (Tag::PhotographicSensitivity, "ISO"),
        (Tag::FocalLength, "Focal length"),
        (Tag::FocalLengthIn35mmFilm, "Focal length (35mm)"),
        (Tag::ExposureBiasValue, "Exposure bias"),
        (Tag::Flash, "Flash"),
        (Tag::WhiteBalance, "White balance"),
        (Tag::Orientation, "Orientation"),
        (Tag::ColorSpace, "Color space"),
        (Tag::Software, "Software"),
        (Tag::Artist, "Artist"),
        (Tag::Copyright, "Copyright"),
    ];
    let mut out = Vec::new();
    for (tag, label) in wanted {
        if let Some(f) = ex.get_field(*tag, In::PRIMARY) {
            let v = f.display_value().with_unit(&ex).to_string();
            let v = v.trim_matches('"').trim().to_string();
            if !v.is_empty() {
                out.push((label.to_string(), v));
            }
        }
    }
    if let (Some(lat), Some(lat_ref), Some(lon), Some(lon_ref)) = (
        ex.get_field(Tag::GPSLatitude, In::PRIMARY),
        ex.get_field(Tag::GPSLatitudeRef, In::PRIMARY),
        ex.get_field(Tag::GPSLongitude, In::PRIMARY),
        ex.get_field(Tag::GPSLongitudeRef, In::PRIMARY),
    ) {
        if let (Some(a), Some(b)) = (dms(&lat.value), dms(&lon.value)) {
            let sa = if lat_ref.display_value().to_string().contains('S') { -a } else { a };
            let sb = if lon_ref.display_value().to_string().contains('W') { -b } else { b };
            out.push(("GPS".into(), format!("{sa:.6}, {sb:.6}")));
        }
    }
    out
}

fn dms(v: &exif::Value) -> Option<f64> {
    if let exif::Value::Rational(r) = v {
        if r.len() >= 3 {
            return Some(r[0].to_f64() + r[1].to_f64() / 60.0 + r[2].to_f64() / 3600.0);
        }
    }
    None
}

/// Decode to an image suitable for the webview. Returns (mime, bytes).
pub fn render(path: &Path, kind: &str, max: u32) -> Res<(String, Vec<u8>)> {
    match kind {
        "raw" => raw_preview(path).map(|b| ("image/jpeg".to_string(), b)),
        "psd" => encode_png(psd_composite(path)?, max),
        _ => {
            let img = image::ImageReader::open(path)
                .or_str()?
                .with_guessed_format()
                .or_str()?;
            let mut img = img;
            img.no_limits();
            let decoded = img.decode().ctx("Could not decode image")?;
            encode_png(decoded, max)
        }
    }
}

fn encode_png(img: image::DynamicImage, max: u32) -> Res<(String, Vec<u8>)> {
    let max = max.clamp(256, 8192);
    let img = if img.width() > max || img.height() > max {
        img.thumbnail(max, max)
    } else {
        img
    };
    // HDR/EXR and 16-bit images become 8-bit RGBA; floats are clamped (simple tone map).
    let img = match img {
        image::DynamicImage::ImageRgb32F(_) | image::DynamicImage::ImageRgba32F(_) => {
            let mut f = img.to_rgba32f();
            for p in f.pixels_mut() {
                for c in 0..3 {
                    // Reinhard + gamma for a pleasant HDR preview.
                    let v = p.0[c].max(0.0);
                    p.0[c] = (v / (1.0 + v)).powf(1.0 / 2.2);
                }
            }
            image::DynamicImage::ImageRgba32F(f).to_rgba8().into()
        }
        image::DynamicImage::ImageRgb8(_) | image::DynamicImage::ImageRgba8(_) | image::DynamicImage::ImageLuma8(_) | image::DynamicImage::ImageLumaA8(_) => img,
        other => other.to_rgba8().into(),
    };
    let mut out = Vec::new();
    let enc = image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    );
    img.write_with_encoder(enc).ctx("Could not encode preview")?;
    Ok(("image/png".into(), out))
}

/// Camera RAW files embed full-size JPEG previews. Find the largest valid one.
pub fn raw_preview(path: &Path) -> Res<Vec<u8>> {
    let mut data = Vec::new();
    std::fs::File::open(path).or_str()?.take(512 << 20).read_to_end(&mut data).or_str()?;
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0xD8 && data[i + 2] == 0xFF {
            if let Some(end) = jpeg_end(&data, i) {
                let len = end - i;
                if len > 16 * 1024 && best.is_none_or(|(_, l)| len > l) {
                    best = Some((i, len));
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    let (s, l) = best.ok_or("No embedded preview found in this RAW file")?;
    Ok(data[s..s + l].to_vec())
}

/// Walk JPEG segments starting at SOI; return the offset just past EOI.
fn jpeg_end(d: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 2;
    let mut saw_sof = false;
    loop {
        // Skip fill bytes.
        while i < d.len() && d[i] == 0xFF && d.get(i + 1) == Some(&0xFF) {
            i += 1;
        }
        if i + 4 > d.len() || d[i] != 0xFF {
            return None;
        }
        let m = d[i + 1];
        match m {
            0xD9 => return saw_sof.then_some(i + 2),
            0xD0..=0xD7 | 0x01 => i += 2,
            0xDA => {
                // Start of scan: skip header then entropy-coded data until a real marker.
                let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
                i += 2 + len;
                while i + 1 < d.len() {
                    if d[i] == 0xFF {
                        let n = d[i + 1];
                        if n == 0x00 || (0xD0..=0xD7).contains(&n) || n == 0xFF {
                            i += if n == 0xFF { 1 } else { 2 };
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
            }
            _ => {
                if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC {
                    saw_sof = true;
                }
                let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
                if len < 2 {
                    return None;
                }
                i += 2 + len;
            }
        }
    }
}

struct PsdHeader {
    version: u16,
    channels: u16,
    height: u32,
    width: u32,
    depth: u16,
    mode: u16,
}

fn psd_header(path: &Path) -> Res<PsdHeader> {
    let mut b = [0u8; 26];
    std::fs::File::open(path).or_str()?.read_exact(&mut b).or_str()?;
    parse_psd_header(&b)
}

fn parse_psd_header(b: &[u8]) -> Res<PsdHeader> {
    if b.len() < 26 || &b[0..4] != b"8BPS" {
        return Err("Not a Photoshop file".into());
    }
    let u16b = |o: usize| u16::from_be_bytes([b[o], b[o + 1]]);
    let u32b = |o: usize| u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    Ok(PsdHeader {
        version: u16b(4),
        channels: u16b(12),
        height: u32b(14),
        width: u32b(18),
        depth: u16b(22),
        mode: u16b(24),
    })
}

/// Decode the flattened composite stored at the end of PSD/PSB files.
fn psd_composite(path: &Path) -> Res<image::DynamicImage> {
    let mut d = Vec::new();
    std::fs::File::open(path).or_str()?.take(2 << 30).read_to_end(&mut d).or_str()?;
    let h = parse_psd_header(&d)?;
    let psb = h.version == 2;
    let (w, ht) = (h.width as usize, h.height as usize);
    if w == 0 || ht == 0 || w * ht > 400_000_000 {
        return Err("Unsupported image size".into());
    }
    if h.depth != 8 && h.depth != 16 {
        return Err(format!("{}-bit PSD files aren't supported", h.depth));
    }
    let rd32 = |o: usize| -> Res<usize> {
        d.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]) as usize).ok_or_else(|| "truncated PSD".to_string())
    };
    let rd64 = |o: usize| -> Res<usize> {
        d.get(o..o + 8).map(|x| u64::from_be_bytes(x.try_into().unwrap()) as usize).ok_or_else(|| "truncated PSD".to_string())
    };
    let mut o = 26;
    o += 4 + rd32(o)?; // color mode data
    o += 4 + rd32(o)?; // image resources
    o += if psb { 8 + rd64(o)? } else { 4 + rd32(o)? }; // layer & mask info
    let comp = d.get(o..o + 2).map(|x| u16::from_be_bytes([x[0], x[1]])).ok_or("truncated PSD")?;
    o += 2;
    let bps = (h.depth / 8) as usize;
    let row_bytes = w * bps;
    let channels = h.channels as usize;
    let use_ch = match h.mode {
        3 => channels.min(4),     // RGB(+A)
        4 => channels.min(5),     // CMYK(+A)
        1 | 8 => channels.min(2), // grayscale / duotone (+A)
        m => return Err(format!("PSD colour mode {m} isn't supported")),
    };
    let mut planes: Vec<Vec<u8>> = Vec::with_capacity(use_ch);
    match comp {
        0 => {
            for c in 0..use_ch {
                let start = o + c * row_bytes * ht;
                let plane = d.get(start..start + row_bytes * ht).ok_or("truncated PSD")?;
                planes.push(plane.to_vec());
            }
        }
        1 => {
            let count_size = if psb { 4 } else { 2 };
            let mut counts = Vec::with_capacity(channels * ht);
            for i in 0..channels * ht {
                let p = o + i * count_size;
                let c = if psb { rd32(p)? } else { d.get(p..p + 2).map(|x| u16::from_be_bytes([x[0], x[1]]) as usize).ok_or("truncated PSD")? };
                counts.push(c);
            }
            let mut pos = o + channels * ht * count_size;
            for c in 0..channels {
                let mut plane = Vec::with_capacity(row_bytes * ht);
                for r in 0..ht {
                    let n = counts[c * ht + r];
                    let src = d.get(pos..pos + n).ok_or("truncated PSD")?;
                    packbits(src, &mut plane, row_bytes);
                    pos += n;
                }
                if c < use_ch {
                    planes.push(plane);
                }
            }
        }
        _ => return Err("This PSD uses ZIP compression for its composite, which isn't supported".into()),
    }
    let px = |plane: &[u8], i: usize| -> u8 { plane.get(i * bps).copied().unwrap_or(0) };
    let mut rgba = vec![255u8; w * ht * 4];
    for i in 0..w * ht {
        let (r, g, b, a) = match h.mode {
            3 => (px(&planes[0], i), px(&planes[1.min(use_ch - 1)], i), px(&planes[2.min(use_ch - 1)], i), if use_ch > 3 { px(&planes[3], i) } else { 255 }),
            4 => {
                // PSD stores CMYK inverted.
                let c = 255 - px(&planes[0], i) as u32;
                let m = 255 - px(&planes[1], i) as u32;
                let y = 255 - px(&planes[2], i) as u32;
                let k = 255 - px(&planes[3.min(use_ch - 1)], i) as u32;
                let conv = |x: u32| (((255 - x) * (255 - k)) / 255) as u8;
                (conv(c), conv(m), conv(y), if use_ch > 4 { px(&planes[4], i) } else { 255 })
            }
            _ => {
                let v = px(&planes[0], i);
                (v, v, v, if use_ch > 1 { px(&planes[1], i) } else { 255 })
            }
        };
        rgba[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, a]);
    }
    let buf = image::RgbaImage::from_raw(w as u32, ht as u32, rgba).ok_or("bad PSD buffer")?;
    Ok(image::DynamicImage::ImageRgba8(buf))
}

fn packbits(src: &[u8], out: &mut Vec<u8>, row_len: usize) {
    let target = out.len() + row_len;
    let mut i = 0;
    while i < src.len() && out.len() < target {
        let n = src[i] as i8;
        i += 1;
        if n >= 0 {
            let cnt = n as usize + 1;
            let end = (i + cnt).min(src.len());
            out.extend_from_slice(&src[i..end]);
            i = end;
        } else if n != -128 {
            let cnt = (1 - n as isize) as usize;
            if let Some(&b) = src.get(i) {
                out.extend(std::iter::repeat_n(b, cnt));
            }
            i += 1;
        }
    }
    out.resize(target, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packbits_decodes() {
        let mut out = Vec::new();
        // literal 3 bytes, then repeat 0xAA 4 times
        packbits(&[2, 1, 2, 3, 0xFD, 0xAA], &mut out, 7);
        assert_eq!(out, vec![1, 2, 3, 0xAA, 0xAA, 0xAA, 0xAA]);
    }

    #[test]
    fn finds_embedded_jpeg() {
        // Build a fake RAW: junk + a tiny real JPEG padded with APP segment to pass the size filter.
        let mut jpeg = Vec::new();
        image::DynamicImage::new_rgb8(64, 64)
            .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .unwrap();
        let mut padded = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x7F, 0xFF];
        padded.extend(std::iter::repeat_n(0u8, 0x7FFF - 2));
        padded.extend_from_slice(&jpeg[2..]);
        let mut raw = b"IIRO\x08\x00junkjunk".to_vec();
        raw.extend_from_slice(&padded);
        raw.extend_from_slice(b"trailing");
        let p = std::env::temp_dir().join(format!("alook-{}.cr2", std::process::id()));
        std::fs::write(&p, &raw).unwrap();
        let got = raw_preview(&p).unwrap();
        assert_eq!(got, padded);
        assert!(image::load_from_memory(&got).is_ok());
    }

    #[test]
    fn psd_round_trip() {
        // 2x1 RGB, raw (uncompressed) composite.
        let mut d = b"8BPS\x00\x01\x00\x00\x00\x00\x00\x00".to_vec();
        d.extend_from_slice(&3u16.to_be_bytes());
        d.extend_from_slice(&1u32.to_be_bytes());
        d.extend_from_slice(&2u32.to_be_bytes());
        d.extend_from_slice(&8u16.to_be_bytes());
        d.extend_from_slice(&3u16.to_be_bytes());
        d.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // 3 empty sections
        d.extend_from_slice(&0u16.to_be_bytes());
        d.extend_from_slice(&[255, 0, 0, 255, 0, 0]); // R, G, B planes
        let p = std::env::temp_dir().join(format!("alook-{}.psd", std::process::id()));
        std::fs::write(&p, &d).unwrap();
        let img = psd_composite(&p).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(1, 0).0, [0, 255, 0, 255]);
    }
}
