use std::io::{self, Read, Seek};
use std::path::Path;

use zip::ZipArchive;

use crate::zip_index::{self, Entry, ZipError};

const MAX_HEADER_BYTES: u64 = 512 * 1024;
const MAX_HEADER_READS: usize = 40;
/// Превью больше этого не читаем целиком: защита от «zip-бомбы» в чужом архиве.
pub const MAX_PREVIEW_BYTES: u64 = 30 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewPick {
    pub raw_name: String,
    pub extension: String,
    pub width: u32,
    pub height: u32,
}

pub fn pick(path: &Path) -> Result<Option<PreviewPick>, ZipError> {
    let index = zip_index::read_index(path)?;
    if index.entries.is_empty() {
        return Ok(None);
    }
    let mut archive = crate::zip_index::open_archive(path)?;
    let mut reads = 0;

    if let Some(pick) = pick_default(&mut archive, &index.entries, &mut reads)? {
        return Ok(Some(pick));
    }
    if let Some(pick) = pick_companions(&mut archive, &index.entries, &mut reads)? {
        return Ok(Some(pick));
    }
    if let Some(pick) = pick_mod_info(&mut archive, &index.entries, &mut reads)? {
        return Ok(Some(pick));
    }
    if let Some(pick) = pick_level(&mut archive, &index.entries, &mut reads)? {
        return Ok(Some(pick));
    }
    if let Some(pick) = pick_app(&mut archive, &index.entries, &mut reads)? {
        return Ok(Some(pick));
    }
    pick_any(&mut archive, &index.entries, &mut reads)
}

pub fn extract(path: &Path, raw_name: &str) -> Result<Vec<u8>, ZipError> {
    let mut archive = crate::zip_index::open_archive(path)?;
    let entry = archive.by_name(raw_name)?;
    read_limited(entry, MAX_PREVIEW_BYTES).map_err(ZipError::Io)
}

fn read_limited(reader: impl Read, max_bytes: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(max_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "картинка превью слишком большая",
        ));
    }
    Ok(bytes)
}

fn pick_default<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entries: &[Entry],
    reads: &mut usize,
) -> Result<Option<PreviewPick>, ZipError> {
    let mut candidates: Vec<&Entry> = entries
        .iter()
        .filter(|entry| {
            let parts: Vec<&str> = entry.name.split('/').collect();
            parts.len() == 3
                && parts[0].eq_ignore_ascii_case("vehicles")
                && is_image(&entry.name)
                && parts[2].to_lowercase().starts_with("default.")
        })
        .collect();
    candidates.sort_by_key(|entry| image_rank(&entry.name));
    for entry in candidates {
        if let Some(pick) = header(archive, entry, 64, reads)? {
            return Ok(Some(pick));
        }
    }
    Ok(None)
}

fn pick_companions<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entries: &[Entry],
    reads: &mut usize,
) -> Result<Option<PreviewPick>, ZipError> {
    let pcs: Vec<&Entry> = entries
        .iter()
        .filter(|entry| {
            let parts: Vec<&str> = entry.name.split('/').collect();
            parts.len() == 3
                && parts[0].eq_ignore_ascii_case("vehicles")
                && entry.name.to_lowercase().ends_with(".pc")
        })
        .collect();

    for pc in pcs {
        let stem = pc.name.trim_end_matches(".pc");
        let candidates: Vec<&Entry> = entries
            .iter()
            .filter(|entry| {
                let entry_stem = entry
                    .name
                    .rsplit_once('.')
                    .map(|(stem, _)| stem)
                    .unwrap_or(&entry.name);
                is_image(&entry.name) && entry_stem.eq_ignore_ascii_case(stem) && entry.depth == 3
            })
            .collect();
        for entry in candidates {
            if let Some(pick) = header(archive, entry, 64, reads)? {
                return Ok(Some(pick));
            }
        }
    }
    Ok(None)
}

fn pick_mod_info<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entries: &[Entry],
    reads: &mut usize,
) -> Result<Option<PreviewPick>, ZipError> {
    let images: Vec<&Entry> = entries
        .iter()
        .filter(|entry| {
            let parts: Vec<&str> = entry.name.split('/').collect();
            parts.len() == 4
                && parts[0].eq_ignore_ascii_case("mod_info")
                && parts[2].eq_ignore_ascii_case("images")
                && is_image(&entry.name)
        })
        .collect();
    for entry in images {
        if let Some(pick) = header(archive, entry, 64, reads)? {
            return Ok(Some(pick));
        }
    }

    let icons: Vec<&Entry> = entries
        .iter()
        .filter(|entry| {
            let parts: Vec<&str> = entry.name.split('/').collect();
            parts.len() == 3
                && parts[0].eq_ignore_ascii_case("mod_info")
                && parts[2].to_lowercase().starts_with("icon.")
                && is_image(&entry.name)
        })
        .collect();
    for entry in icons {
        if let Some(pick) = header(archive, entry, 64, reads)? {
            return Ok(Some(pick));
        }
    }
    Ok(None)
}

fn pick_level<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entries: &[Entry],
    reads: &mut usize,
) -> Result<Option<PreviewPick>, ZipError> {
    let mut candidates: Vec<&Entry> = entries
        .iter()
        .filter(|entry| {
            let parts: Vec<&str> = entry.name.split('/').collect();
            parts.len() == 3
                && parts[0].eq_ignore_ascii_case("levels")
                && is_image(&entry.name)
                && !is_texture(&entry.name)
        })
        .collect();
    candidates.sort_by_key(|entry| {
        (
            !entry.name.to_lowercase().contains("preview"),
            image_rank(&entry.name),
        )
    });
    for entry in candidates {
        if let Some(pick) = header(archive, entry, 64, reads)? {
            return Ok(Some(pick));
        }
    }
    Ok(None)
}

fn pick_app<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entries: &[Entry],
    reads: &mut usize,
) -> Result<Option<PreviewPick>, ZipError> {
    let candidates: Vec<&Entry> = entries
        .iter()
        .filter(|entry| {
            let parts: Vec<&str> = entry.name.split('/').collect();
            parts.len() == 5
                && parts[0].eq_ignore_ascii_case("ui")
                && parts[1].eq_ignore_ascii_case("modules")
                && parts[2].eq_ignore_ascii_case("apps")
                && parts[4].to_lowercase().starts_with("app.")
                && is_image(&entry.name)
        })
        .collect();
    for entry in candidates {
        if let Some(pick) = header(archive, entry, 48, reads)? {
            return Ok(Some(pick));
        }
    }
    Ok(None)
}

fn pick_any<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entries: &[Entry],
    reads: &mut usize,
) -> Result<Option<PreviewPick>, ZipError> {
    let mut candidates: Vec<&Entry> = entries
        .iter()
        .filter(|entry| is_image(&entry.name) && !is_texture(&entry.name))
        .collect();
    candidates.sort_by(|a, b| {
        let a_png = a.name.to_lowercase().ends_with(".png");
        let b_png = b.name.to_lowercase().ends_with(".png");
        a_png
            .cmp(&b_png)
            .then_with(|| a.depth.cmp(&b.depth))
            .then_with(|| b.size.cmp(&a.size))
            .then_with(|| a.name.cmp(&b.name))
    });
    for entry in candidates {
        if let Some(pick) = header(archive, entry, 200, reads)? {
            return Ok(Some(pick));
        }
    }
    Ok(None)
}

fn header<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry: &Entry,
    min_size: u32,
    reads: &mut usize,
) -> Result<Option<PreviewPick>, ZipError> {
    if *reads >= MAX_HEADER_READS {
        return Ok(None);
    }
    *reads += 1;
    let Ok(mut file) = archive.by_name(&entry.raw_name) else {
        return Ok(None);
    };
    let limit = entry.size.min(MAX_HEADER_BYTES) as usize;
    let mut bytes = vec![0u8; limit];
    let read = file.read(&mut bytes)?;
    bytes.truncate(read);
    let Some((width, height)) = image_size(&bytes) else {
        return Ok(None);
    };
    if width < min_size || height < min_size {
        return Ok(None);
    }
    Ok(Some(PreviewPick {
        raw_name: entry.raw_name.clone(),
        extension: extension(&entry.name),
        width,
        height,
    }))
}

fn image_rank(name: &str) -> u8 {
    let lower = name.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        0
    } else if lower.ends_with(".png") {
        1
    } else {
        2
    }
}

fn is_image(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".png")
}

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, extension)| extension.to_lowercase())
        .unwrap_or_else(|| "jpg".to_string())
}

fn is_texture(name: &str) -> bool {
    let lower = name.to_lowercase();
    let folders = [
        "/art/",
        "/textures/",
        "/txt/",
        "/skins/",
        "/paint/",
        "/bakes/",
        "/decals/",
        "/materials/",
    ];
    if folders.iter().any(|folder| lower.contains(folder)) {
        return true;
    }
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    ["_nm", "_diff", "_bake_", "corona"]
        .iter()
        .any(|marker| file.contains(marker))
}

fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() >= 24 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a] {
        let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        return Some((width, height));
    }
    if bytes.len() >= 4 && bytes[0] == 0xff && bytes[1] == 0xd8 {
        let mut pos = 2;
        while pos + 4 <= bytes.len() {
            if bytes[pos] != 0xff {
                pos += 1;
                continue;
            }
            let marker = bytes[pos + 1];
            if marker == 0xd8 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
                pos += 2;
                continue;
            }
            if pos + 4 > bytes.len() {
                return None;
            }
            let length = u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]) as usize;
            if length < 2 || pos + 2 + length > bytes.len() {
                return None;
            }
            if (0xc0..=0xcf).contains(&marker)
                && !matches!(marker, 0xc4 | 0xc8 | 0xcc)
                && length >= 7
            {
                let height = u16::from_be_bytes([bytes[pos + 5], bytes[pos + 6]]) as u32;
                let width = u16::from_be_bytes([bytes[pos + 7], bytes[pos + 8]]) as u32;
                return Some((width, height));
            }
            pos += 2 + length;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes
    }

    fn jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8];
        bytes.extend_from_slice(&[
            0xff,
            0xc0,
            0x00,
            0x11,
            0x08,
            (height >> 8) as u8,
            height as u8,
            (width >> 8) as u8,
            width as u8,
            0x03,
        ]);
        bytes.extend_from_slice(&[0x01, 0x11, 0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00]);
        bytes
    }

    fn build_zip(path: &Path, files: &[(&str, Vec<u8>)]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn prefers_default_vehicle_image() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        build_zip(
            &path,
            &[
                ("vehicles/car/default.png", png(800, 600)),
                ("vehicles/car/default.jpg", jpeg(1920, 1080)),
                ("vehicles/car/bakes/texture.jpg", jpeg(2048, 2048)),
            ],
        );
        let pick = pick(&path).unwrap().unwrap();
        assert_eq!(pick.raw_name, "vehicles/car/default.jpg");
        assert_eq!((pick.width, pick.height), (1920, 1080));
    }

    #[test]
    fn skips_tiny_and_texture_images() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        build_zip(
            &path,
            &[
                ("vehicles/car/default.png", png(63, 63)),
                ("textures/cover.png", png(900, 500)),
                ("cover.png", png(900, 500)),
            ],
        );
        let pick = pick(&path).unwrap().unwrap();
        assert_eq!(pick.raw_name, "cover.png");
    }

    #[test]
    fn picks_app_icon() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        build_zip(&path, &[("ui/modules/apps/myapp/app.png", png(64, 64))]);
        let pick = pick(&path).unwrap().unwrap();
        assert_eq!(pick.raw_name, "ui/modules/apps/myapp/app.png");
    }

    #[test]
    fn extracts_same_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        let bytes = jpeg(640, 480);
        build_zip(&path, &[("cover.jpg", bytes.clone())]);
        assert_eq!(extract(&path, "cover.jpg").unwrap(), bytes);
    }

    #[test]
    fn no_images_returns_none() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        build_zip(&path, &[("scripts/main.lua", b"x".to_vec())]);
        assert!(pick(&path).unwrap().is_none());
    }

    #[test]
    fn extraction_stops_at_the_preview_limit() {
        assert_eq!(read_limited(&b"1234"[..], 4).unwrap(), b"1234");
        let error = read_limited(&b"12345"[..], 4).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
