use std::path::{Path, PathBuf};

/// 拡張子が画像かどうかを判定する
pub fn is_image_ext(name: &str) -> bool {
    let Some(pos) = name.rfind('.') else { return false };
    let ext = &name[pos + 1..];
    ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") ||
    ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("webp") ||
    ext.eq_ignore_ascii_case("gif") || ext.eq_ignore_ascii_case("bmp") ||
    ext.eq_ignore_ascii_case("avif")
}

/// パスの区切り文字を Windows 形式に統一し、UNCパスを正規化する
pub fn clean_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    let mut p = PathBuf::from(s.replace('/', "\\"));

    // UNCパス (\\?\) の除去
    let s_cleaned = p.to_string_lossy();
    if let Some(stripped) = s_cleaned.strip_prefix(r"\\?\") {
        p = PathBuf::from(stripped);
    }

    // ドライブレターの末尾に \ を追加 (C: -> C:\)
    if p.to_string_lossy().ends_with(':') {
        p = PathBuf::from(format!("{}\\", p.display()));
    }
    p
}

/// パスを「\\?\」プリフィックスを含まないクリーンな Windows 形式の文字列に変換する。
/// 外部アプリやエクスプローラーに渡すパスは必ずこの関数を通した String を使用する。
pub fn to_clean_string(path: &Path) -> String {
    let s = path.to_string_lossy();
    let cleaned = if let Some(stripped) = s.strip_prefix(r"\\?\") {
        stripped.to_string()
    } else {
        s.into_owned()
    };
    cleaned.replace('/', "\\")
}

/// ファイルサイズを表示用に整形する
pub fn format_size(size: u64) -> String {
    if size >= 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:.0} KB", size as f64 / 1024.0)
    } else {
        format!("{} B", size)
    }
}

/// 表示用にパスからファイル名（またはルート名）を抽出する
pub fn get_display_name(path: &Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// アーカイブのパスとエントリ名を結合して、OS標準の形式（Windowsなら \）で返す
pub fn join_entry_path(archive_path: &Path, entry_name: &str) -> String {
    let full = if archive_path.is_dir() {
        archive_path.join(entry_name)
    } else {
        PathBuf::from(format!("{}\\{}", archive_path.display(), entry_name.replace('/', "\\")))
    };
    clean_path(&full).to_string_lossy().into_owned()
}

/// ファイルの種類を検出する
#[derive(Debug, Clone, PartialEq)]
pub enum ArchiveKind {
    Zip,
    SevenZ,
    Pdf,
    Plain,
}

pub fn detect_kind(path: &Path) -> ArchiveKind {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else { return ArchiveKind::Plain; };
    if ext.eq_ignore_ascii_case("zip") {
        ArchiveKind::Zip
    } else if ext.eq_ignore_ascii_case("7z") {
        ArchiveKind::SevenZ
    } else if ext.eq_ignore_ascii_case("pdf") {
        ArchiveKind::Pdf
    } else {
        ArchiveKind::Plain
    }
}

/// パスが隠し属性（Hidden）を持っているか判定する
/// Windows以外ではドットファイル（.）を隠しファイルとみなす
pub fn is_hidden(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        // ドライブのルート（"C:\" 等）は常に表示する
        if path.parent().is_none() {
            return false;
        }

        use std::os::windows::fs::MetadataExt;
        std::fs::metadata(path).map(|m| m.file_attributes() & 0x2 != 0).unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    path.file_name().map(|n| n.to_string_lossy().starts_with('.')).unwrap_or(false)
}

/// パスがWindowsのシステム属性（System）を持っているか判定する
pub fn is_system(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        // ドライブのルート（"C:\" 等）は常に表示する
        if path.parent().is_none() {
            return false;
        }

        use std::os::windows::fs::MetadataExt;
        match std::fs::metadata(path) {
            Ok(m) => {
                let attr = m.file_attributes();
                // システム属性 (0x4) があれば非表示
                attr & 0x4 != 0
            }
            // メタデータが取れない場合は、ドライブ等の特殊なパスの可能性があるため表示する
            Err(_) => false,
        }
    }
    #[cfg(not(target_os = "windows"))]
    false
}

fn is_leap_year(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

fn days_in_month(year: u64, month: u64) -> u64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 31,
    }
}

fn unix_days_to_ymd(days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    let mut d = days;
    loop {
        let dy = if is_leap_year(year) { 366 } else { 365 };
        if d < dy { break; }
        d -= dy;
        year += 1;
    }
    let mut month = 1u64;
    while month < 12 {
        let dm = days_in_month(year, month);
        if d < dm { break; }
        d -= dm;
        month += 1;
    }
    (year, month, d + 1)
}

/// 秒（UNIXタイム）を yyyy/mm/dd 形式の文字列に変換する (chrono 依存排除用)
pub fn format_timestamp(secs: u64) -> String {
    if secs == 0 { return "----/--/--".to_string(); }
    let (y, m, d) = unix_days_to_ymd(secs / 86400);
    format!("{:04}/{:02}/{:02}", y, m, d)
}

/// 秒（UNIXタイム）を yyyymmdd 形式で返す（イースターエッグ用）
#[allow(dead_code)]
pub fn format_date_compact(secs: u64) -> String {
    if secs == 0 { return "00000000".to_string(); }
    let (y, m, d) = unix_days_to_ymd(secs / 86400);
    format!("{:04}{:02}{:02}", y, m, d)
}

// ── 自然順ソート ─────────────────────────────────────────────────────────────

pub fn natord(a: &str, b: &str) -> std::cmp::Ordering {
    let a = basename(a);
    let b = basename(b);
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek(), bi.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let na: u64 = consume_num(&mut ai);
                let nb: u64 = consume_num(&mut bi);
                match na.cmp(&nb) {
                    std::cmp::Ordering::Equal => {}
                    other => return other,
                }
            }
            _ => {
                let ac = ai.next().and_then(|c| c.to_lowercase().next()).unwrap_or('\0');
                let bc = bi.next().and_then(|c| c.to_lowercase().next()).unwrap_or('\0');
                match ac.cmp(&bc) {
                    std::cmp::Ordering::Equal => {}
                    other => return other,
                }
            }
        }
    }
}

fn consume_num(iter: &mut std::iter::Peekable<std::str::Chars>) -> u64 {
    let mut n: u64 = 0;
    while let Some(&c) = iter.peek() {
        if let Some(digit) = c.to_digit(10) {
            n = n.saturating_mul(10).saturating_add(digit as u64);
            iter.next();
        } else {
            break;
        }
    }
    n
}

fn basename(s: &str) -> &str {
    Path::new(s)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(s)
}