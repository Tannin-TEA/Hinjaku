use crate::error::{HinjakuError, Result};
use std::io::Read;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use pdfium_render::prelude::*;

use crate::utils; // utils モジュールをインポート

// ArchiveReader トレイトの定義
pub trait ArchiveReader: Send + Sync {
    fn list_images(&self, path: &Path) -> Result<Vec<ImageEntry>>;
    fn read_entry(&self, archive_path: &Path, entry_name: &str, entry_index: Option<usize>) -> Result<Vec<u8>>;
    fn list_nav_targets(&self, path: &Path) -> Result<Vec<PathBuf>>;
    fn get_roots(&self) -> Vec<PathBuf>;
}

#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub name: String,
    pub mtime: u64,
    pub size: u64,
    pub archive_index: usize,
}

// DefaultArchiveReader の実装
pub struct DefaultArchiveReader;

impl ArchiveReader for DefaultArchiveReader {
    fn list_images(&self, path: &Path) -> Result<Vec<ImageEntry>> {
        match utils::detect_kind(path) {
            utils::ArchiveKind::Zip => self.list_zip(path),
            utils::ArchiveKind::SevenZ => self.list_7z(path),
            utils::ArchiveKind::Pdf => self.list_pdf(path),
            utils::ArchiveKind::Plain => self.list_plain(path),
        }
    }

    fn read_entry(&self, archive_path: &Path, entry_name: &str, entry_index: Option<usize>) -> Result<Vec<u8>> {
        match utils::detect_kind(archive_path) {
            utils::ArchiveKind::Zip => self.read_zip(archive_path, entry_name, entry_index),
            utils::ArchiveKind::SevenZ => self.read_7z(archive_path, entry_name),
            utils::ArchiveKind::Pdf => self.read_pdf(archive_path, entry_index),
            utils::ArchiveKind::Plain => {
                let target = if archive_path.is_file() { archive_path.to_path_buf() } else { archive_path.join(entry_name) };
                Ok(std::fs::read(target)?)
            }
        }
    }

    fn list_nav_targets(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let mut targets = Vec::new();
        if !path.is_dir() { return Ok(targets); }
        for entry in std::fs::read_dir(path)? {
            let e = entry?;
            
            // システム属性または隠し属性を持つパスをスキップ (WindowsではDirEntryから属性を直接取得して高速化)
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::fs::MetadataExt;
                if let Ok(m) = e.metadata() {
                    let attr = m.file_attributes();
                    // 0x4: System, 0x2: Hidden
                    if (attr & 0x4 != 0) || (attr & 0x2 != 0) { continue; }
                }
            }
            #[cfg(not(target_os = "windows"))]
            if utils::is_system(&e.path()) || utils::is_hidden(&e.path()) { continue; }

            let p = e.path();

            let kind = utils::detect_kind(&p);
            if p.is_dir() || matches!(kind, utils::ArchiveKind::Zip | utils::ArchiveKind::SevenZ | utils::ArchiveKind::Pdf) {
                targets.push(utils::clean_path(&p));
            }
        }
        targets.sort_by(|a, b| utils::natord(&a.to_string_lossy(), &b.to_string_lossy()));
        Ok(targets)
    }

    fn get_roots(&self) -> Vec<PathBuf> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
            let mut roots = Vec::new();
            let drives = unsafe { GetLogicalDrives() };
            for i in 0..26 {
                if (drives >> i) & 1 != 0 {
                    let p = PathBuf::from(format!("{}:\\", (b'A' + i as u8) as char));
                    roots.push(utils::clean_path(&p));
                }
            }
            roots
        }
        #[cfg(not(windows))]
        {
            vec![PathBuf::from("/")]
        }
    }
}

// DefaultArchiveReader のプライベートヘルパー関数
impl DefaultArchiveReader {
    fn init_pdfium(&self) -> Result<Pdfium> {
        // スレッドごとにバインディングを保持するのは難しいため、
        // DLLの探索パスの解決を効率化します。
        let library_path = if let Ok(exe_p) = std::env::current_exe() {
            exe_p.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("./"))
        } else {
            PathBuf::from("./")
        };

        let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(library_path.to_str().unwrap_or("./")))
            .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./")))
            .map_err(|e| HinjakuError::Archive(format!("Pdfium init error: {e}")))?;

        Ok(Pdfium::new(bindings))
    }

    fn list_zip(&self, path: &Path) -> Result<Vec<ImageEntry>> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        let mut zip = zip::ZipArchive::new(reader)?;

        // 1. file_names() を使って画像ファイルだけのインデックスを先に抽出する。
        // 中央ディレクトリの文字列参照だけで判定するため、画像以外のファイルに対して
        // メタデータ解析（by_index）を行うコストを完全にスキップできます。
        let image_indices: Vec<usize> = zip.file_names()
            .enumerate()
            .filter(|(_, name)| utils::is_image_ext(name))
            .map(|(i, _)| i)
            .collect();

        let mut entries = Vec::with_capacity(image_indices.len());
        for i in image_indices {
            let entry = zip.by_index(i)?;
                // zip::DateTime (Option) から UNIX タイムへの簡易変換（Chrono 依存を排除）
                let t_opt = entry.last_modified();
                let mtime = if let Some(t) = t_opt {
                    if t.year() >= 1980 {
                        let years = (t.year() as u64).saturating_sub(1970);
                        let months = t.month() as u64;
                        let days = t.day() as u64;
                        // 相対的なソート順序を維持するための概算計算
                        (years * 31536000) + (months * 2592000) + (days * 86400)
                    } else { 0 }
                } else { 0 };

                entries.push(ImageEntry {
                    name: entry.name().to_string(), // 画像確定後のみ String を作成
                    mtime,
                    size: entry.size(),
                    archive_index: i,
                });
        }
        Ok(entries)
    }

    fn read_zip(&self, path: &Path, entry_name: &str, entry_index: Option<usize>) -> Result<Vec<u8>> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        let mut zip = zip::ZipArchive::new(reader)?;
        let mut entry = if let Some(idx) = entry_index {
            zip.by_index(idx)?
        } else {
            zip.by_name(entry_name)?
        };
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        Ok(buf)
    }

// ── 7z ──────────────────────────────────────────────────────────────────────

    fn list_7z(&self, path: &Path) -> Result<Vec<ImageEntry>> {
        let mut entries = Vec::new();
        let file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        let mut reader = BufReader::new(file);
        
        let archive = sevenz_rust::Archive::read(&mut reader, len, &[])
            .map_err(|e| HinjakuError::Archive(format!("7z read error: {e}")))?;

        for entry in &archive.files {
            let name = entry.name().to_string();
            if !entry.is_directory() && utils::is_image_ext(&name) {
                let mtime = (u64::from(entry.last_modified_date) / 10_000_000)
                    .saturating_sub(11_644_473_600);
                entries.push(ImageEntry {
                    name,
                    mtime,
                    size: entry.size,
                    archive_index: 0,
                });
            }
        }
        Ok(entries)
    }

    fn read_7z(&self, path: &Path, entry_name: &str) -> Result<Vec<u8>> {
        let mut result: Option<Vec<u8>> = None;
        sevenz_rust::decompress_file_with_extract_fn(path, Path::new("."), |entry, reader, _dest| {
            if entry.name() == entry_name {
                let mut buf = Vec::new();
                reader.read_to_end(&mut buf)?;
                result = Some(buf);
            }
            Ok(false)
        })
        .map_err(|e| HinjakuError::Archive(format!("7z read error: {e}")))?;
        result.ok_or_else(|| HinjakuError::NotFound(format!("Entry not found: {entry_name}")))
    }

// ── PDF ─────────────────────────────────────────────────────────────────────

    fn list_pdf(&self, path: &Path) -> Result<Vec<ImageEntry>> {
        let pdfium = self.init_pdfium()?;
        let document = pdfium.load_pdf_from_file(path, None)
            .map_err(|e| HinjakuError::Archive(format!("PDF load error: {e}")))?;
        
        let mtime = std::fs::metadata(path)?.modified()
            .unwrap_or(std::time::UNIX_EPOCH)
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

        let mut entries = Vec::new();
        for i in 0..document.pages().len() {
            entries.push(ImageEntry {
                name: format!("Page {:04}.pdf", i + 1),
                mtime,
                size: 0,
                archive_index: i as usize,
            });
        }
        Ok(entries)
    }

    fn read_pdf(&self, path: &Path, page_index: Option<usize>) -> Result<Vec<u8>> {
        let pdfium = self.init_pdfium()?;
        
        let document = pdfium.load_pdf_from_file(path, None)
            .map_err(|e| HinjakuError::Archive(format!("PDF load error: {e}")))?;
        
        let index = page_index.unwrap_or(0) as u16;
        let page = document.pages().get(index)
            .map_err(|_| HinjakuError::NotFound(format!("Page {} not found", index)))?;

        // 長辺を 1920px (MAX_TEX_DIM) に合わせてレンダリング
        let width = page.width().value;
        let height = page.height().value;
        let scale = 1920.0 / width.max(height);
        let render_w = (width * scale) as i32;
        let render_h = (height * scale) as i32;

        let bitmap = page.render(render_w, render_h, None)
            .map_err(|e| HinjakuError::Archive(format!("PDF render error: {e}")))?;
        
        // BMP 形式で書き出す。PNG よりもエンコードが圧倒的に速い。
        let mut bmp_data = Vec::new();
        bitmap.as_image() // pdfium-render が内部で適切にピクセル変換を行う
            .write_to(&mut std::io::Cursor::new(&mut bmp_data), ::image::ImageFormat::Bmp)
            .map_err(|e| HinjakuError::Archive(format!("BMP conversion error: {e}")))?;

        Ok(bmp_data)
    }

// ── Plain folder / single file ──────────────────────────────────────────────

    fn list_plain(&self, path: &Path) -> Result<Vec<ImageEntry>> {
        // 単一ファイルが渡された場合は親ディレクトリのリストを返す
        if path.is_file() {
            let dir = path.parent().ok_or_else(|| HinjakuError::NotFound("No parent dir".to_string()))?;
            return self.list_plain(dir);
        }
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let e = entry?;
            let name_os = e.file_name();
            // ここでも String 生成を避け、参照 (&str) で判定
            let Some(name_str) = name_os.to_str() else { continue; };

            if utils::is_image_ext(name_str) {
                let meta = e.metadata()?;
                let mtime = meta
                    .modified()
                    .unwrap_or(std::time::UNIX_EPOCH)
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                entries.push(ImageEntry {
                    name: name_str.to_string(),
                    mtime,
                    size: meta.len(),
                    archive_index: 0,
                });
            }
        }
        Ok(entries)
    }
}
