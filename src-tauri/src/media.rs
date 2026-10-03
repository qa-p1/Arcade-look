//! Audio metadata and cover art via lofty.

use crate::util::{OrStr, Res};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::{Accessor, ItemKey};
use lofty::tag::Tag;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<String>,
    pub track: Option<u32>,
    pub track_total: Option<u32>,
    pub disc: Option<u32>,
    pub composer: Option<String>,
    pub comment: Option<String>,
    pub duration_ms: Option<u64>,
    pub bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub bit_depth: Option<u8>,
    pub has_cover: bool,
}

fn best_tag(f: &lofty::file::TaggedFile) -> Option<&Tag> {
    f.primary_tag().or_else(|| f.first_tag())
}

pub fn info(path: &Path) -> Res<AudioInfo> {
    let f = lofty::probe::Probe::open(path)
        .or_str()?
        .guess_file_type()
        .or_str()?
        .read()
        .ctx("Could not read audio metadata")?;
    let p = f.properties();
    let mut ai = AudioInfo {
        duration_ms: Some(p.duration().as_millis() as u64).filter(|d| *d > 0),
        bitrate: p.audio_bitrate().or(p.overall_bitrate()),
        sample_rate: p.sample_rate(),
        channels: p.channels(),
        bit_depth: p.bit_depth(),
        ..Default::default()
    };
    if let Some(t) = best_tag(&f) {
        let s = |v: Option<std::borrow::Cow<str>>| v.map(|c| c.trim().to_string()).filter(|s| !s.is_empty());
        ai.title = s(t.title());
        ai.artist = s(t.artist());
        ai.album = s(t.album());
        ai.genre = s(t.genre());
        ai.comment = s(t.comment());
        ai.track = t.track();
        ai.track_total = t.track_total();
        ai.disc = t.disk();
        ai.year = t.date().map(|d| d.year.to_string());
        ai.album_artist = t.get_string(ItemKey::AlbumArtist).map(str::to_string);
        ai.composer = t.get_string(ItemKey::Composer).map(str::to_string);
        ai.has_cover = !t.pictures().is_empty();
    }
    if !ai.has_cover {
        ai.has_cover = f.tags().iter().any(|t| !t.pictures().is_empty());
    }
    Ok(ai)
}

/// Returns (mime, bytes) of the front cover (or first picture).
pub fn cover(path: &Path) -> Res<(String, Vec<u8>)> {
    let f = lofty::read_from_path(path).or_str()?;
    for t in f.tags() {
        let pics = t.pictures();
        let pic = pics
            .iter()
            .find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront)
            .or_else(|| pics.first());
        if let Some(p) = pic {
            let mime = p
                .mime_type()
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "image/jpeg".into());
            return Ok((mime, p.data().to_vec()));
        }
    }
    Err("no cover art".into())
}
