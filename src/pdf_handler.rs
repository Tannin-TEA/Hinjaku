use crate::error::{HinjakuError, Result};
use crate::archive::ImageEntry;
use std::path::Path;
use std::sync::OnceLock;
use pdfium_render::prelude::*;

/// PDFium の DLL が存在するディレクトリの検索結果をキャッシュする。
/// Pdfium 構造体自体は Sync ではないため、パスだけを共有する。
static PDFIUM_PATH: OnceLock<std::result::Result<std::path::PathBuf, String>> = OnceLock::new();

fn get_pdfium() -> Result<Pdfium> {
    let path_res = PDFIUM_PATH.get_or_init(|| {
        let library_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("./"));

        // 実際にロード可能か試行して、成功したパスを保存する
        if Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(library_path.to_str().unwrap_or("./"))).is_ok() {
            Ok(library_path)
        } else if Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./")).is_ok() {
            Ok(std::path::PathBuf::from("./"))
        } else {
            Err("pdfium.dll が見つからないか、読み込めません。\n公式: https://pdfium.googlesource.com/pdfium/\nDL先: https://github.com/bblanchon/pdfium-binaries".to_string())
        }
    });

    let path = path_res.as_ref().map_err(|e| HinjakuError::Archive(e.clone()))?;
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(path.to_str().unwrap_or("./")))
        .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./")))
        .map_err(|_| HinjakuError::Archive("PDFium binding failed".to_string()))?;

    Ok(Pdfium::new(bindings))
}

pub fn list_pdf(path: &Path) -> Result<Vec<ImageEntry>> {
    let pdfium = get_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| HinjakuError::Archive(format!("PDF load error: {e}")))?;

    let mtime = std::fs::metadata(path)?
        .modified()
        .unwrap_or(std::time::UNIX_EPOCH)
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let count = document.pages().len();
    let mut entries = Vec::with_capacity(count as usize);
    for i in 0..count {
        entries.push(ImageEntry {
            name: format!("Page {:04}.pdf", i + 1),
            mtime,
            size: 0,
            archive_index: i as usize,
        });
    }
    Ok(entries)
}

pub fn read_pdf(path: &Path, page_index: Option<usize>, dpi: u32) -> Result<Vec<u8>> {
    let pdfium = get_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| HinjakuError::Archive(format!("PDF load error: {e}")))?;

    let index = page_index.unwrap_or(0) as u16;
    let page = document
        .pages()
        .get(index)
        .map_err(|_| HinjakuError::NotFound(format!("Page {} not found", index)))?;

    let width = page.width().value;
    let height = page.height().value;

    if width <= 0.0 || height <= 0.0 {
        return Err(HinjakuError::Archive(
            "PDF ページのサイズが不正です。".to_string(),
        ));
    }

    let scale = (dpi as f32 / 72.0).min(10.0);
    let render_w = ((width * scale) as i32).clamp(1, 8192);
    let render_h = ((height * scale) as i32).clamp(1, 8192);

    let bitmap = page
        .render(render_w, render_h, None)
        .map_err(|e| HinjakuError::Archive(format!("PDF render error: {e}")))?;

    let mut bmp_data = Vec::new();
    bitmap
        .as_image()
        .write_to(
            &mut std::io::Cursor::new(&mut bmp_data),
            ::image::ImageFormat::Bmp,
        )
        .map_err(|e| HinjakuError::Archive(format!("BMP conversion error: {e}")))?;

    Ok(bmp_data)
}
