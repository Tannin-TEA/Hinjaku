use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

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

/// WM_COPYDATA を使って既存のウィンドウにパスを送信する
pub fn send_path_via_wm_copydata(window_title: &str, path: &Path) {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SendMessageW, WM_COPYDATA, COPYDATASTRUCT};
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let title_wide: Vec<u16> = OsStr::new(window_title).encode_wide().chain(Some(0)).collect();
        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if hwnd != 0 {
                let path_str = path.to_string_lossy().to_string();
                let bytes = path_str.as_bytes();
                let cds = COPYDATASTRUCT {
                    dwData: 0,
                    cbData: bytes.len() as u32,
                    lpData: bytes.as_ptr() as *mut _,
                };
                SendMessageW(hwnd, WM_COPYDATA, 0, &cds as *const _ as _);
            }
        }
    }
}

static mut GLOBAL_TX: Option<mpsc::Sender<PathBuf>> = None;
static mut OLD_WNDPROC: isize = 0;

/// Windowsメッセージをフックしてパス受信を待機する
pub fn install_message_hook(ctx: &egui::Context) -> Receiver<PathBuf> {
    let (tx, rx) = mpsc::channel();
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, CallWindowProcW, GWLP_WNDPROC, WM_COPYDATA, COPYDATASTRUCT, FindWindowW};
        use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM, LRESULT};

        GLOBAL_TX = Some(tx);
        
        // 自身のウィンドウハンドルを取得 (タイトルで検索)
        // ※起動直後は main.rs で設定した "Hinjaku" というタイトルになっている前提
        let title_wide: Vec<u16> = "Hinjaku\0".encode_utf16().collect();
        let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
        
        if hwnd != 0 {
            OLD_WNDPROC = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
            
            unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
                if msg == WM_COPYDATA {
                    let cds = lparam as *const COPYDATASTRUCT;
                    if !cds.is_null() {
                        let slice = std::slice::from_raw_parts((*cds).lpData as *const u8, (*cds).cbData as usize);
                        if let Ok(path_str) = std::str::from_utf8(slice) {
                            if let Some(tx) = &GLOBAL_TX {
                                let _ = tx.send(PathBuf::from(path_str));
                            }
                        }
                    }
                    return 1; // 処理済み
                }
                CallWindowProcW(OLD_WNDPROC, hwnd, msg, wparam, lparam)
            }
            
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc as isize);
        }
    }
    rx
}

/// 外部アプリを起動する
pub fn launch_external(exe: &str, args_tmpl: &[String], path_p: &str, path_a: &str) -> Result<(), String> {
    if exe.is_empty() { return Err("実行プログラムが指定されていません。".to_string()); }

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
            let ret = ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                exe_u16.as_ptr(),
                parameters.as_ptr(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
            if ret as isize <= 32 {
                return Err(match ret as isize {
                    2 => format!("ファイルが見つかりません: {}", exe),
                    3 => format!("パスが見つかりません: {}", exe),
                    5 => "アクセスが拒否されました。".to_string(),
                    31 => "指定された拡張子の関連付けがありません。".to_string(),
                    _ => format!("外部アプリの起動に失敗しました (Code: {})", ret as isize),
                });
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        if !Path::new(exe).exists() { return; }
        let mut cmd = std::process::Command::new(exe);
        if args_tmpl.is_empty() {
            cmd.arg(path_p);
        } else {
            for arg in args_tmpl {
                let replaced = arg.replace("%P", path_p).replace("%A", path_a);
                cmd.arg(replaced);
            }
        }
        match cmd.spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("起動に失敗しました: {}", e)),
        }
    }
}

/// エクスプローラーで選択状態で表示する
pub fn reveal_in_explorer(path: &Path) -> Result<(), String> {
    if !path.exists() { return Err("対象のパスが見つかりません。".to_string()); }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let explorer: Vec<u16> = "explorer.exe\0".encode_utf16().collect();
        let parameters: Vec<u16> = format!("/select,\"{}\"\0", path.display()).encode_utf16().collect();

        unsafe {
            let ret = ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                explorer.as_ptr(),
                parameters.as_ptr(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
            if ret as isize <= 32 {
                return Err(format!("エクスプローラーの起動に失敗しました (Code: {})", ret as isize));
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        match std::process::Command::new("open").arg(path).spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("エクスプローラー起動失敗: {}", e)),
        }
    }
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

/// 現在のプロセスのメモリ使用量（ワーキングセット）を文字列で取得する
pub fn get_memory_usage_str() -> String {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        use std::mem;

        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { mem::zeroed() };
        let process = unsafe { GetCurrentProcess() };
        let size = mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if unsafe { GetProcessMemoryInfo(process, &mut counters, size) } != 0 {
            let bytes = counters.WorkingSetSize as u64;
            return format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0));
        }
    }
    "--- MB".to_string()
}