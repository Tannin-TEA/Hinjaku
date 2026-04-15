use std::path::{Path, PathBuf};
use std::process::Command;

/// コマンドライン引数を解析して (INI名, 対象パス) を返す
pub fn parse_args(args: &[String]) -> (Option<String>, Option<PathBuf>) {
    let mut config_name = None;
    let mut path_arg = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-c" | "--config" => {
                if i + 1 < args.len() {
                    config_name = Some(args[i + 1].clone());
                    i += 2;
                } else { i += 1; }
            }
            _ if !args[i].starts_with('-') && path_arg.is_none() => {
                path_arg = Some(PathBuf::from(&args[i]));
                i += 1;
            }
            _ => i += 1,
        }
    }
    (config_name, path_arg)
}

/// Windows環境での二重起動防止チェック
pub fn check_single_instance() -> Option<isize> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
        use windows_sys::Win32::System::Threading::CreateMutexW;
        let name: Vec<u16> = "Local\\Hinjaku-Unique-Mutex-Name\0".encode_utf16().collect();
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
            if GetLastError() == ERROR_ALREADY_EXISTS { return None; }
            Some(handle as isize)
        }
    }
    #[cfg(not(target_os = "windows"))]
    { Some(0) }
}

/// 外部アプリを起動する
pub fn launch_external(exe: &str, args_tmpl: &[String], path_p: &str, path_a: &str) {
    if exe.is_empty() { return; }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let exe_u16: Vec<u16> = format!("{}\0", exe).encode_utf16().collect();

        // 引数の組み立て。引数がない場合はパスをダブルクォートで囲って渡す。
        let params_str = if args_tmpl.is_empty() {
            format!("\"{}\"", path_p)
        } else {
            args_tmpl.iter()
                .map(|arg| arg.replace("%P", path_p).replace("%A", path_a))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let parameters: Vec<u16> = format!("{}\0", params_str).encode_utf16().collect();

        unsafe {
            ShellExecuteW(0, operation.as_ptr(), exe_u16.as_ptr(), parameters.as_ptr(), std::ptr::null(), SW_SHOWNORMAL);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if !Path::new(exe).exists() { return; }
        let mut cmd = Command::new(exe);
        if args_tmpl.is_empty() {
            cmd.arg(path_p);
        } else {
            for arg in args_tmpl {
                let replaced = arg.replace("%P", path_p).replace("%A", path_a);
                cmd.arg(replaced);
            }
        }
        let _ = cmd.spawn();
    }
}

/// エクスプローラーで選択状態で表示する
pub fn reveal_in_explorer(path: &Path) {
    if !path.exists() { return; }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        // ShellExecuteW を使用してエクスプローラーを呼び出します。
        // これは OS のシェルサービスを介するため、Command::spawn で直接バイナリを叩くよりも
        // エクスプローラーの既存インスタンスとの統合がスムーズになり、プロセスの重複や残留を防げます。
        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let explorer: Vec<u16> = "explorer.exe\0".encode_utf16().collect();
        let parameters: Vec<u16> = format!("/select,\"{}\"\0", path.display()).encode_utf16().collect();

        unsafe {
            ShellExecuteW(0, operation.as_ptr(), explorer.as_ptr(), parameters.as_ptr(), std::ptr::null(), SW_SHOWNORMAL);
        }
    }
    #[cfg(not(target_os = "windows"))]
    { let _ = Command::new("open").arg(path).spawn(); }
}

/// H字型のアイコンデータを生成
pub fn create_h_icon() -> eframe::egui::IconData {
    let size = 32usize;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) * 4;
            let is_h = (x >= 6 && x <= 10 && y >= 5 && y <= 26) ||
                       (x >= 21 && x <= 25 && y >= 5 && y <= 26) ||
                       (y >= 14 && y <= 17 && x > 10 && x < 21);
            if is_h { rgba[i..i+4].copy_from_slice(&[255, 255, 255, 255]); }
        }
    }
    eframe::egui::IconData { rgba, width: size as u32, height: size as u32 }
}

/// 秒（UNIXタイム）を yyyy/mm/dd 形式の文字列に変換する (chrono 依存排除用)
pub fn format_timestamp(secs: u64) -> String {
    if secs == 0 { return "----/--/--".to_string(); }
    let days = secs / 86400;
    let year = 1970 + (days / 365); // 概算。ソート基準として秒単位の数値(mtime)は保持されているため表示用。
    let month = ((days % 365) / 30) + 1;
    let day = (days % 30) + 1;
    format!("{:04}/{:02}/{:02}", year, month.min(12), day.min(31))
}