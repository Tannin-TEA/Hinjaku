use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicIsize, Ordering};
use eframe::egui;

#[cfg(target_os = "windows")]
use std::os::windows::ffi::{OsStrExt, OsStringExt};

/// コマンドライン引数を解析して (INI名, 対象パス) を返す
pub fn parse_args(args: &[String]) -> (Option<String>, Option<PathBuf>, bool, Option<String>) {
    let mut config_name = None;
    let mut path_arg = None;
    let mut debug_mode = false;
    let mut renderer_override = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-c" | "--config" => {
                if i + 1 < args.len() {
                    config_name = Some(args[i + 1].clone());
                    i += 2;
                } else { i += 1; }
            }
            "-d" | "--debug" => {
                debug_mode = true;
                i += 1;
            }
            "-W" => {
                renderer_override = Some("wgpu".to_string());
                i += 1;
            }
            "-O" => {
                renderer_override = Some("glow".to_string());
                i += 1;
            }
            _ if !args[i].starts_with('-') && path_arg.is_none() => {
                path_arg = Some(PathBuf::from(&args[i]));
                i += 1;
            }
            _ => i += 1,
        }
    }
    (config_name, path_arg, debug_mode, renderer_override)
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

/// WindowsのGUIアプリとして起動しつつ、起動元のコンソールに出力できるようにする
pub fn setup_console() {
    #[cfg(target_os = "windows")]
    unsafe {
        // 親プロセスのコンソールへのアタッチを試みる（ターミナルから起動された場合など）
        if windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS) == 0 {
            // 失敗した場合は新しくコンソールウィンドウを割り当てる（エクスプローラから起動された場合など）
            windows_sys::Win32::System::Console::AllocConsole();
        }
    }
}

/// Hinjakuであることを識別するための定数
const HJK_COPYDATA_ID: usize = 0x484A4B; // "HJK"

/// WM_COPYDATA を使って既存のウィンドウにパスを送信する
pub fn send_path_via_wm_copydata(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_COPYDATA, EnumWindows, GetWindowTextW, GetWindowThreadProcessId};
        use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
        use windows_sys::Win32::Foundation::{HWND, LPARAM};
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;

        struct Target { hwnd: HWND, found: bool, self_pid: u32 }
        let mut target = Target { hwnd: std::ptr::null_mut(), found: false, self_pid: unsafe { GetCurrentProcessId() } };

        // Hinjakuで始まるタイトルのウィンドウを列挙して探す
        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
            let target = &mut *(lparam as *mut Target);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == target.self_pid { return 1; } // 自分自身はスキップ

            let mut text = [0u16; 512];
            let len = GetWindowTextW(hwnd, text.as_mut_ptr(), 512);
            if len > 0 {
                let title = String::from_utf16_lossy(&text[..len as usize]);
                if title.starts_with("Hinjaku") {
                    target.hwnd = hwnd;
                    target.found = true;
                    return 0; // 中断
                }
            }
            1 // 続行
        }

        unsafe {
            EnumWindows(Some(enum_proc), &mut target as *mut _ as _);

            if target.found {
                // 既存プロセスへ送るパスをクリーンな文字列として確定
                let path_str = crate::utils::to_clean_string(path);
                let path_u16: Vec<u16> = std::ffi::OsStr::new(&path_str).encode_wide().collect();

                let cds = COPYDATASTRUCT {
                    dwData: HJK_COPYDATA_ID,
                    cbData: (path_u16.len() * 2) as u32, // バイト数なので2倍
                    lpData: path_u16.as_ptr() as *mut _,
                };
                SendMessageW(target.hwnd, WM_COPYDATA, 0, &cds as *const _ as _);
            }
        }
    }
}

static GLOBAL_TX: OnceLock<mpsc::Sender<PathBuf>> = OnceLock::new();
static GLOBAL_CTX: OnceLock<egui::Context> = OnceLock::new();
static OLD_WNDPROC: AtomicIsize = AtomicIsize::new(0);

#[cfg(target_os = "windows")]
unsafe extern "system" fn wnd_proc(hwnd: windows_sys::Win32::Foundation::HWND, msg: u32, wparam: windows_sys::Win32::Foundation::WPARAM, lparam: windows_sys::Win32::Foundation::LPARAM) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_COPYDATA, CallWindowProcW};
    use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
    use std::mem::transmute;

    if msg == WM_COPYDATA {
        let cds = lparam as *const COPYDATASTRUCT;
        if !cds.is_null() && (*cds).dwData == HJK_COPYDATA_ID {
            // 受信したバイナリを UTF-16 スライスとして解釈
            let len = ((*cds).cbData / 2) as usize;
            let u16_slice = std::slice::from_raw_parts((*cds).lpData as *const u16, len);
            let os_str = std::ffi::OsString::from_wide(u16_slice);
            if let Some(tx) = GLOBAL_TX.get() {
                let _ = tx.send(PathBuf::from(os_str));
                // OSメッセージを受け取った瞬間に egui を叩き起こして update を走らせる
                if let Some(ctx) = GLOBAL_CTX.get() {
                    ctx.request_repaint();
                }
            }
            return 1;
        }
    }
    CallWindowProcW(transmute(OLD_WNDPROC.load(Ordering::SeqCst)), hwnd, msg, wparam, lparam)
}

/// Windowsメッセージをフックしてパス受信を待機する
pub fn install_message_hook(ctx: &egui::Context, window_title: &str) -> Receiver<PathBuf> {
    let (tx, rx) = mpsc::channel();
    
    // OnceLock への値のセット (.set() を使用)
    let _ = GLOBAL_TX.set(tx);
    let _ = GLOBAL_CTX.set(ctx.clone());

    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWLP_WNDPROC, FindWindowW};
        // 自身のウィンドウハンドルを取得。
        // タイトルが動的に変わっている可能性があるため、
        // main.rs で生成した起動時のタイトルを使用して特定する。
        let title_wide: Vec<u16> = std::ffi::OsStr::new(window_title).encode_wide().chain(Some(0)).collect();
        let mut hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
        
        // もし見つからない場合は "Hinjaku" 単体で再試行
        if hwnd.is_null() {
            hwnd = FindWindowW(std::ptr::null(), "Hinjaku\0".encode_utf16().collect::<Vec<_>>().as_ptr());
        }

        if !hwnd.is_null() {
            OLD_WNDPROC.store(GetWindowLongPtrW(hwnd, GWLP_WNDPROC), Ordering::SeqCst);
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc as *const () as isize);
        }
    }
    rx
}

/// 外部アプリを起動する
pub fn launch_external(exe: &str, args_tmpl: &[String], path_v: &str, path_p: &str) -> Result<(), String> {
    if exe.is_empty() { return Err("実行プログラムが指定されていません。".to_string()); }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let exe_u16: Vec<u16> = format!("{}\0", exe).encode_utf16().collect();

        // 引用符の付与のみを行う。内部でのパス変換（置換）は行わない。
        let quote = |s: &str| {
            let s = s.trim_matches('"');
            format!("\"{}\"", s)
        };

        let params_str = if args_tmpl.is_empty() {
            quote(path_v)
        } else {
            args_tmpl.iter()
                .map(|arg| {
                    arg.replace("%P", &quote(path_v))
                       .replace("%p", &quote(path_v))
                       .replace("%F", &quote(path_v))
                       .replace("%f", &quote(path_v))
                       .replace("%A", &quote(path_p))
                       .replace("%a", &quote(path_p))
                       .replace("%D", &quote(path_p))
                       .replace("%d", &quote(path_p))
                })
                .collect::<Vec<_>>().join(" ")
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
        if !Path::new(exe).exists() { return Err(format!("実行ファイルが見つかりません: {}", exe)); }
        let mut cmd = std::process::Command::new(exe);
        if args_tmpl.is_empty() {
            cmd.arg(path_p);
        } else {
            for arg in args_tmpl {
                let replaced = arg.replace("%P", path_p).replace("%p", path_p)
                                  .replace("%A", path_a).replace("%a", path_a);
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

        // エクスプローラーは UNCパスを受け付けないため、確実にクリーンな形式で渡す
        let cleaned = crate::utils::to_clean_string(path);
        let parameters: Vec<u16> = format!("/select,\"{}\"\0", cleaned).encode_utf16().collect();

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

/// プライマリモニターの解像度を取得する
pub fn get_primary_monitor_size() -> (f32, f32) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        (GetSystemMetrics(SM_CXSCREEN) as f32, GetSystemMetrics(SM_CYSCREEN) as f32)
    }
    #[cfg(not(target_os = "windows"))]
    (1920.0, 1080.0)
}

/// Windowsのメモリマッピングを使用してフォントファイルを読み込む
/// 物理メモリへのコピーを避け、OSのページキャッシュに管理を委ねる
#[cfg(target_os = "windows")]
pub fn mmap_font_file(path: &str) -> Option<&'static [u8]> {
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, GetFileSizeEx};
    use windows_sys::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, PAGE_READONLY, FILE_MAP_READ};
    use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, CloseHandle, GENERIC_READ};

    unsafe {
        let path_u16: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let handle = CreateFileW(path_u16.as_ptr(), GENERIC_READ, FILE_SHARE_READ, std::ptr::null(), OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, std::ptr::null_mut());
        if handle == INVALID_HANDLE_VALUE { return None; }

        let mut size = 0i64;
        if GetFileSizeEx(handle, &mut size) == 0 {
            CloseHandle(handle);
            return None;
        }

        let mapping = CreateFileMappingW(handle, std::ptr::null(), PAGE_READONLY, 0, 0, std::ptr::null());
        if mapping == std::ptr::null_mut() {
            CloseHandle(handle);
            return None;
        }

        let ptr = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0);
        if ptr.Value.is_null() {
            CloseHandle(mapping);
            CloseHandle(handle);
            return None;
        }

        // アプリ起動中はずっと使用するため、ハンドルをリーク（保持）させて静的参照として返す
        Some(std::slice::from_raw_parts(ptr.Value as *const u8, size as usize))
    }
}