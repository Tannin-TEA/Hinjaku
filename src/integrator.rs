use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicIsize, Ordering};
use eframe::egui;

#[cfg(target_os = "windows")]
use std::os::windows::ffi::{OsStrExt, OsStringExt};

/// Hinjakuであることを識別するための定数
const HJK_COPYDATA_ID: usize = 0x484A4B; // "HJK"

/// WM_COPYDATA を使って既存のウィンドウにパスを送信する
pub fn send_path_via_wm_copydata(path: &Path) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_COPYDATA, EnumWindows, GetWindowTextW};
        use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
        use windows_sys::Win32::Foundation::{HWND, LPARAM};

        struct Search { hwnd: HWND }
        unsafe extern "system" fn find_hinjaku(hwnd: HWND, lparam: LPARAM) -> i32 {
            let s = &mut *(lparam as *mut Search);
            let mut buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 512);
            if len > 0 {
                // null文字を除去して解釈
                let title = String::from_utf16_lossy(&buf[..len as usize]);
                if title.starts_with("Hinjaku") {
                    s.hwnd = hwnd;
                    return 0; // 見つかったので中断
                }
            }
            1 // 続行
        }
        let mut s = Search { hwnd: std::ptr::null_mut() };
        EnumWindows(Some(find_hinjaku), &mut s as *mut _ as LPARAM);
        let hwnd = s.hwnd;

        if !hwnd.is_null() {
            let path_str = crate::utils::to_clean_string(path);
            if !path_str.is_empty() {
                let path_u16: Vec<u16> = std::ffi::OsStr::new(&path_str).encode_wide().collect();
                let cds = COPYDATASTRUCT {
                    dwData: HJK_COPYDATA_ID,
                    cbData: (path_u16.len() * 2) as u32, // バイト数なので2倍
                    lpData: path_u16.as_ptr() as *mut _,
                };
                SendMessageW(hwnd, WM_COPYDATA, 0, &cds as *const _ as _);
            }
        }
    }
}
/// 外部からのパスを受け取るためのチャネル。PathBuf と、それが外部プロセスからのものか (true) どうか (false) を送る。
static GLOBAL_TX: OnceLock<mpsc::Sender<(PathBuf, bool)>> = OnceLock::new();
static GLOBAL_CTX: OnceLock<egui::Context> = OnceLock::new();
static OLD_WNDPROC: AtomicIsize = AtomicIsize::new(0);

#[cfg(target_os = "windows")]
unsafe extern "system" fn wnd_proc(hwnd: windows_sys::Win32::Foundation::HWND, msg: u32, wparam: windows_sys::Win32::Foundation::WPARAM, lparam: windows_sys::Win32::Foundation::LPARAM) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_COPYDATA, CallWindowProcW};
    use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
    use std::mem::transmute;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    // パニックが FFI 境界（OS側）に漏れないよう防壁を設置
    let result = catch_unwind(AssertUnwindSafe(|| {
        if msg == WM_COPYDATA {
            let cds = lparam as *const COPYDATASTRUCT;
            if !cds.is_null() && (*cds).dwData == HJK_COPYDATA_ID {
                let len = ((*cds).cbData / 2) as usize;
                if len > 0 && !(*cds).lpData.is_null() {
                    // 受信したバイナリを UTF-16 スライスとして安全に解釈
                    let u16_slice = std::slice::from_raw_parts((*cds).lpData as *const u16, len);
                    let os_str = std::ffi::OsString::from_wide(u16_slice);
                    if let Some(tx) = GLOBAL_TX.get() { // WM_COPYDATA は外部からのパスなので true
                        let _ = tx.send((PathBuf::from(os_str), true));
                        if let Some(ctx) = GLOBAL_CTX.get() {
                            ctx.request_repaint();
                        }
                    }
                }
                return Some(1); // 処理済み(TRUE)
            }
        }
        None
    }));

    match result {
        Ok(Some(ret)) => ret, // 正常に処理された場合
        Ok(None) => {
            // メッセージが WM_COPYDATA 以外なら元のプロシージャを呼ぶ
            CallWindowProcW(transmute(OLD_WNDPROC.load(Ordering::SeqCst)), hwnd, msg, wparam, lparam)
        }
        Err(e) => {
            // パニック発生時。標準エラーに内容を出し、安全に 0 (FALSE) を返して終了
            eprintln!("Hinjaku integration error: Panic detected in WndProc. Unwind aborted. {:?}", e);
            0
        }
    }
}

/// IPC (WM_COPYDATA) および D&D イベントを処理するためのチャネルをセットアップし、
/// Windowsメッセージフックをインストールする。
///
/// 戻り値: (Sender, Receiver) - Sender は D&D イベントを内部から送るために使用し、Receiver は全てのパスイベントを受け取る。
pub fn setup_ipc_channels(ctx: &egui::Context) -> (mpsc::Sender<(PathBuf, bool)>, Receiver<(PathBuf, bool)>) {
    let (tx, rx) = mpsc::channel();
    
    // OnceLock への値のセット (.set() を使用)
    let _ = GLOBAL_TX.set(tx.clone());
    let _ = GLOBAL_CTX.set(ctx.clone());

    #[cfg(target_os = "windows")]
    {
        // フックのインストールは、ウィンドウハンドルが確定してから行う必要があるため、
        // `try_install_hook` を別途用意し、App::update で毎フレーム試行する。
        // ここではチャネルのセットアップのみ。
    }
    (tx, rx)
}

/// Windowsメッセージをフックする。ウィンドウハンドルが取得できたら一度だけ成功する。
/// 成功したら true を返す。
#[cfg(target_os = "windows")]
pub fn try_install_hook() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWLP_WNDPROC, EnumWindows, GetWindowThreadProcessId, GetWindowRect};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};

    unsafe {
        let target_pid = GetCurrentProcessId();
        struct Search { pid: u32, hwnd: HWND }
        unsafe extern "system" fn find_own_window(hwnd: HWND, lparam: LPARAM) -> i32 {
            let s = &mut *(lparam as *mut Search);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == s.pid {
                let mut rect: RECT = std::mem::zeroed();
                // 自身のPIDかつ、ある程度のサイズがあるウィンドウをメインウィンドウとみなす
                if GetWindowRect(hwnd, &mut rect) != 0 && (rect.right - rect.left) > 10 {
                    s.hwnd = hwnd;
                    return 0;
                }
            }
            1
        }
        let mut s = Search { pid: target_pid, hwnd: std::ptr::null_mut() };
        EnumWindows(Some(find_own_window), &mut s as *mut _ as LPARAM);

        if s.hwnd.is_null() { return false; }
        let hwnd = s.hwnd;

        // 既にフック済みかチェック (OLD_WNDPROC が 0 でないならフック済み)
        if OLD_WNDPROC.load(Ordering::SeqCst) != 0 { return true; }
        OLD_WNDPROC.store(GetWindowLongPtrW(hwnd, GWLP_WNDPROC), Ordering::SeqCst);
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc as *const () as isize);
        true
    }
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
                let replaced = arg.replace("%P", path_v).replace("%p", path_v)
                                  .replace("%F", path_v).replace("%f", path_v)
                                  .replace("%A", path_p).replace("%a", path_p)
                                  .replace("%D", path_p).replace("%d", path_p);
                cmd.arg(replaced);
            }
        }
        match cmd.spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("起動に失敗しました: {}", e)),
        }
    }
}



/// 現在のプロセスのメモリ使用量（ワーキングセット）を文字列で取得する
pub fn get_memory_usage_str() -> String {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        use std::mem;

        let mut counters: PROCESS_MEMORY_COUNTERS_EX = unsafe { mem::zeroed() };
        let process = unsafe { GetCurrentProcess() };
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        if unsafe { GetProcessMemoryInfo(process, &mut counters as *mut _ as *mut _, size) } != 0 {
            // WorkingSetSize = 共有DLL含む全ページ。タスクマネージャー「メモリ」列より多く見えるが正確。
            let bytes = counters.WorkingSetSize as u64;
            return format!("RAM: {:.1} MB", bytes as f64 / (1024.0 * 1024.0));
        }
    }
    "RAM: --- MB".to_string()
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