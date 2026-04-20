use crate::error::{HinjakuError, Result};
use crate::archive::ImageEntry;
use std::path::Path;
use pdfium_render::prelude::*;

fn init_pdfium() -> Result<Pdfium> {
    let library_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("./"));

    let bindings =
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
            library_path.to_str().unwrap_or("./"),
        ))
        .or_else(|_| {
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
        })
        .map_err(|_| {
            HinjakuError::Archive("pdfium.dll が見つからないか、読み込めません。".to_string())
        })?;

    Ok(Pdfium::new(bindings))
}

pub fn list_pdf(path: &Path) -> Result<Vec<ImageEntry>> {
    let pdfium = init_pdfium()?;
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

pub fn read_pdf(path: &Path, page_index: Option<usize>, max_dim: u32) -> Result<Vec<u8>> {
    let pdfium = init_pdfium()?;
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

    let scale = (max_dim as f32 / width.max(height)).min(10.0);
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
