use std::io::Read;
use std::sync::Arc;
use ::image::AnimationDecoder;
use crate::{archive::ArchiveReader, utils};
use crate::config::FilterMode;
use crate::constants::*;
use super::{LoadRequest, FrameData, Rotation};

/// バイト列・アーカイブ種別に応じて画像データをデコードし FrameData として返す
pub(super) fn process_load_request(
    req: &LoadRequest,
    zip_cache: &mut Option<(std::path::PathBuf, zip::ZipArchive<std::fs::File>)>,
    archive_reader: &Arc<dyn ArchiveReader>,
) -> std::result::Result<Vec<FrameData>, String> {
    let kind = utils::detect_kind(&req.archive_path);
    let limit = req.max_dim;

    let bytes = if let Some(idx) = req.entry_index {
        if matches!(kind, utils::ArchiveKind::Zip) {
            if zip_cache.as_ref().map(|(p, _)| p != &req.archive_path).unwrap_or(true) {
                let file = std::fs::File::open(&req.archive_path).map_err(|e| e.to_string())?;
                // ZipArchive は自身でシークを管理するため、BufReader は不要。File を直接渡す。
                let zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
                *zip_cache = Some((req.archive_path.clone(), zip));
            }
            let (_, ref mut zip) = zip_cache.as_mut().ok_or("Cache error")?;
            let mut entry = zip.by_index(idx).map_err(|e| e.to_string())?;
            let mut buf = Vec::with_capacity(entry.size() as usize);
            if let Err(e) = entry.read_to_end(&mut buf) {
                // CRC不一致はデータ自体は読めているので続行、それ以外はエラー
                if !e.to_string().contains("Invalid checksum") || buf.is_empty() {
                    return Err(e.to_string());
                }
            }
            buf
        } else if matches!(kind, utils::ArchiveKind::Pdf) {
            if zip_cache.is_some() { *zip_cache = None; }
            archive_reader.read_entry(&req.archive_path, &req.entry_name, Some(idx), req.max_dim).map_err(|e| e.to_string())?
        } else {
            archive_reader.read_entry(&req.archive_path, &req.entry_name, Some(idx), limit).map_err(|e| e.to_string())?
        }
    } else {
        // Plainファイルまたは7zアーカイブからの読み込み
        if zip_cache.is_some() { *zip_cache = None; }
        archive_reader.read_entry(&req.archive_path, &req.entry_name, None, limit).map_err(|e| e.to_string())?
    };

    let ext = req.entry_name.to_ascii_lowercase();

    if (ext.ends_with(".gif") || ext.ends_with(".webp")) && bytes.len() <= loading::MAX_ANIM_DECODE_SIZE {
        let frames_res: ::image::ImageResult<Vec<::image::Frame>> = if ext.ends_with(".gif") {
            ::image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes))
                .and_then(|d| d.into_frames().collect::<::image::ImageResult<Vec<_>>>())
        } else {
            ::image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&bytes))
                .and_then(|d| {
                    if d.has_animation() {
                        d.into_frames().collect::<::image::ImageResult<Vec<_>>>()
                    } else {
                        Err(::image::ImageError::IoError(std::io::Error::other(
                            "Not animated WebP",
                        )))
                    }
                })
        };

        if let Ok(frames) = frames_res {
            // メモリ保護: フレーム数が極端に多い場合は静止画として扱う
            if frames.len() > 200 {
                return Ok(vec![FrameData { image: frames[0].clone().into_buffer(), delay_ms: 0 }]);
            }

            let mut result_frames = Vec::new();
            for frame in frames {
                let delay = frame.delay();
                let (n, d) = delay.numer_denom_ms();
                let delay_ms = if d > 0 { n / d } else { loading::DEFAULT_ANIM_FRAME_DELAY_MS };
                let delay_ms = if delay_ms < loading::MIN_ANIM_FRAME_DELAY_MS { loading::DEFAULT_ANIM_FRAME_DELAY_MS } else { delay_ms };

                let img = frame.into_buffer();
                let img = downscale_if_needed(img, limit, req.filter_mode);
                let img = apply_rotation(img, req.rotation);
                result_frames.push(FrameData { image: img, delay_ms });
            }
            return Ok(result_frames);
            // Err(_) はフォールバックして静的画像として扱う
        }
    }

    let img = if ext.ends_with(".avif") {
        #[cfg(target_os = "windows")]
        { crate::wic::decode_rgba(&bytes)? }
        #[cfg(not(target_os = "windows"))]
        { ::image::load_from_memory(&bytes).map_err(|e| e.to_string())?.into_rgba8() }
    } else {
        ::image::load_from_memory(&bytes).map_err(|e| e.to_string())?.into_rgba8()
    };

    let img = if kind == utils::ArchiveKind::Pdf {
        img
    } else {
        downscale_if_needed(img, limit, req.filter_mode)
    };
    let img = apply_rotation(img, req.rotation);

    Ok(vec![FrameData { image: img, delay_ms: 0 }])
}

pub(super) fn downscale_if_needed(img: ::image::RgbaImage, max_dim: u32, filter: FilterMode) -> ::image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim { return img; }
    let scale = (max_dim as f32 / w as f32).min(max_dim as f32 / h as f32);
    let (nw, nh) = (((w as f32 * scale) as u32).max(1), ((h as f32 * scale) as u32).max(1));

    let filter_type = match filter {
        FilterMode::Nearest  => ::image::imageops::FilterType::Nearest,
        FilterMode::Bilinear => ::image::imageops::FilterType::Triangle,
        FilterMode::Bicubic  => ::image::imageops::FilterType::CatmullRom,
        FilterMode::Lanczos  => ::image::imageops::FilterType::Lanczos3,
    };
    ::image::imageops::resize(&img, nw, nh, filter_type)
}

pub(super) fn apply_rotation(img: ::image::RgbaImage, rot: Rotation) -> ::image::RgbaImage {
    match rot {
        Rotation::R0   => img,
        Rotation::R90  => ::image::imageops::rotate90(&img),
        Rotation::R180 => ::image::imageops::rotate180(&img),
        Rotation::R270 => ::image::imageops::rotate270(&img),
    }
}
