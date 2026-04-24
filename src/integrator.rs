use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use eframe::egui;

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStringExt;

/// 既存インスタンスと通信するための名前付きパイプ名
const PIPE_NAME: &str = r"\\.\pipe\Hinjaku-IPC-5e2d9a3f";

/// 名前付きパイプでパスを既存インスタンスに送り、ウィンドウを前面に出す
pub fn send_path_to_existing_instance(path: &Path) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::Storage::FileSystem::{CreateFileW, WriteFile, OPEN_EXISTING};
        use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
        use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE, CloseHandle, HWND, LPARAM};
        use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, SetForegroundWindow, IsIconic, ShowWindow, SW_RESTORE};

        let path_str = crate::utils::to_clean_string(path);
        if path_str.is_empty() { return; }

        // 1. 名前付きパイプでパスを送信
        let pipe_name_w: Vec<u16> = PIPE_NAME.encode_utf16().chain(Some(0u16)).collect();
        if WaitNamedPipeW(pipe_name_w.as_ptr(), 5000) != 0 {
            let h = CreateFileW(
                pipe_name_w.as_ptr(),
                GENERIC_WRITE, 0,
                std::ptr::null(), OPEN_EXISTING, 0,
                std::ptr::null_mut(),
            );
            if h != INVALID_HANDLE_VALUE {
                let bytes: Vec<u8> = path_str.encode_utf16()
                    .flat_map(|c| c.to_le_bytes())
                    .collect();
                let mut written = 0u32;
                if WriteFile(h, bytes.as_ptr().cast(), bytes.len() as u32, &mut written, std::ptr::null_mut()) == 0 {
                    eprintln!("[Hinjaku IPC] パスの送信に失敗しました");
                }
                CloseHandle(h);
            } else {
                eprintln!("[Hinjaku IPC] パイプへの接続に失敗しました");
            }
        } else {
            eprintln!("[Hinjaku IPC] 既存インスタンスのパイプが見つかりません (タイムアウト)");
        }

        // 2. 既存ウィンドウを前面に出す（第2プロセスはフォアグラウンドにいるため SetForegroundWindow が有効）
        struct S { hwnd: HWND }
        unsafe extern "system" fn find_hinjaku(hwnd: HWND, lp: LPARAM) -> i32 {
            let s = &mut *(lp as *mut S);
            let mut buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 512);
            if len > 0 && String::from_utf16_lossy(&buf[..len as usize]).starts_with("Hinjaku") {
                s.hwnd = hwnd;
                return 0;
            }
            1
        }
        let mut s = S { hwnd: std::ptr::null_mut() };
        EnumWindows(Some(find_hinjaku), &mut s as *mut _ as LPARAM);
        if !s.hwnd.is_null() {
            if IsIconic(s.hwnd) != 0 { ShowWindow(s.hwnd, SW_RESTORE); }
            SetForegroundWindow(s.hwnd);
        }
    }
}

/// 名前付きパイプサーバーをバックグラウンドスレッドで起動する。
/// 受信したパスは tx へ送り、ctx で再描画を要求する。
fn start_pipe_server(tx: mpsc::Sender<(PathBuf, bool)>, ctx: egui::Context) {
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || {
            use windows_sys::Win32::System::Pipes::{
                CreateNamedPipeW, ConnectNamedPipe, DisconnectNamedPipe,
                PIPE_TYPE_MESSAGE, PIPE_READMODE_MESSAGE, PIPE_WAIT,
            };
            use windows_sys::Win32::Storage::FileSystem::{ReadFile, PIPE_ACCESS_INBOUND};
            use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, GetLastError, ERROR_PIPE_CONNECTED};

            let pipe_name_w: Vec<u16> = PIPE_NAME.encode_utf16().chain(Some(0u16)).collect();

            loop {
                let pipe = unsafe {
                    CreateNamedPipeW(
                        pipe_name_w.as_ptr(),
                        PIPE_ACCESS_INBOUND,
                        PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                        255,  // 最大インスタンス数
                        0, 8192, 0,
                        std::ptr::null(),
                    )
                };
                if pipe == INVALID_HANDLE_VALUE {
                    eprintln!("[Hinjaku IPC] CreateNamedPipeW 失敗、再試行...");
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }

                // クライアントの接続を待つ（ブロッキング）
                let connected = unsafe {
                    ConnectNamedPipe(pipe, std::ptr::null_mut()) != 0
                        || GetLastError() == ERROR_PIPE_CONNECTED
                };

                if connected {
                    let mut buf = [0u8; 8192];
                    let mut read = 0u32;
                    let ok = unsafe {
                        ReadFile(pipe, buf.as_mut_ptr().cast(), buf.len() as u32, &mut read, std::ptr::null_mut())
                    };
                    if ok != 0 && read >= 2 && read % 2 == 0 {
                        let words: &[u16] = unsafe {
                            std::slice::from_raw_parts(buf.as_ptr().cast(), (read / 2) as usize)
                        };
                        let path = PathBuf::from(std::ffi::OsString::from_wide(words));
                        if tx.send((path, true)).is_ok() {
                            ctx.request_repaint();
                        }
                    }
                }

                unsafe {
                    DisconnectNamedPipe(pipe);
                    CloseHandle(pipe);
                }
            }
        });
    }
}

/// D&D および IPC パスイベントのチャネルをセットアップし、パイプサーバーを起動する。
///
/// 戻り値: (Sender, Receiver) — Sender は D&D を内部から送るために使用
pub fn setup_ipc_channels(ctx: &egui::Context) -> (mpsc::Sender<(PathBuf, bool)>, Receiver<(PathBuf, bool)>) {
    let (tx, rx) = mpsc::channel();
    start_pipe_server(tx.clone(), ctx.clone());
    (tx, rx)
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