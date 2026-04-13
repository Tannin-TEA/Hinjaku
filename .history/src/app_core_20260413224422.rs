use crate::App;
use crate::config;
use crate::types::{ListResult, Rotation, LoadRequest, LoadResult};
use chrono::TimeZone;
use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::io::Read;

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_path: Option<PathBuf>,
        listener: Option<std::net::TcpListener>,
    ) -> Self {
        // 日本語フォント
        let mut fonts = FontDefinitions::default();
        for font_path in &["C:\\Windows\\Fonts\\meiryo.ttc", "C:\\Windows\\Fonts\\msjh.ttc"] {
            if let Ok(bytes) = std::fs::read(font_path) {
                fonts
                    .font_data
                    .insert("japanese".to_owned(), FontData::from_owned(bytes));
                fonts
                    .families
                    .get_mut(&FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "japanese".to_owned());
                fonts
                    .families
                    .get_mut(&FontFamily::Monospace)
                    .unwrap()
                    .insert(0, "japanese".to_owned());
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);

        // バックグラウンドワーカースレッド起動
        let (req_tx, req_rx) = mpsc::channel::<LoadRequest>();
        let (res_tx, res_rx) = mpsc::channel::<LoadResult>();

        let current_idx_shared = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let req_rx = Arc::new(Mutex::new(req_rx));

        // ワーカースレッドを4つ起動して並列デコード
        for _ in 0..4 {
            let rx = Arc::clone(&req_rx);
            let tx = res_tx.clone();
            let worker_idx = Arc::clone(&current_idx_shared);

            std::thread::spawn(move || {
                while let Ok(req) = { rx.lock().unwrap().recv() } {
                    let result = (|| -> Option<LoadResult> {
                        // キューに溜まっている間にページが移動していたらスキップ
                        let current = worker_idx.load(Ordering::Relaxed);
                        let dist = (req.index as isize - current as isize).abs();
                        if dist > 10 {
                            return None;
                        }

                        let bytes = crate::archive::read_entry(&req.archive_path, &req.entry_name).ok()?;
                        let img = image::load_from_memory(&bytes).ok()?.to_rgba8();

                        let img = crate::downscale_if_needed(img, req.max_dim, req.linear_filter);
                        let img = crate::apply_rotation(img, req.rotation);

                        Some(LoadResult {
                            index: req.index,
                            key: req.key,
                            image: img,
                        })
                    })();

                    if let Some(r) = result {
                        let _ = tx.send(r);
                    }
                }
            });
        }

        let (config, config_path) = config::load_config_file();
        let settings_args_tmp = config.external_args.join(" ");

        // パス転送用のチャンネル
        let (path_tx, path_rx) = mpsc::channel();

        // リスト取得用のチャンネル
        let (list_tx, list_rx) = mpsc::channel();

        // リスナーが渡された場合、通信待ち受けスレッドを起動
        if let Some(l) = listener {
            let tx = path_tx.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                for stream in l.incoming() {
                    if let Ok(mut s) = stream {
                        let mut buf = String::new();
                        if s.read_to_string(&mut buf).is_ok() {
                            let _ = tx.send(PathBuf::from(buf.trim()));
                            ctx.request_repaint();
                        }
                    }
                }
            });
        }

        let mut app = Self {
            archive_path: None,
            entries: Vec::new(),
            entries_meta: Vec::new(),
            current: 0,
            target_index: 0,
            cache: std::collections::HashMap::new(),
            cache_lru: std::collections::VecDeque::new(),
            load_tx: req_tx,
            load_rx: res_rx,
            pending: HashSet::new(),
            current_idx_shared,
            wheel_accumulator: 0.0,
            path_rx,
            list_tx,
            list_rx,
            config,
            show_settings: false,
            show_sort_settings: false,
            sort_focus_idx: 0,
            settings_args_tmp,
            config_path,
            is_listing: false,
            is_loading_archive: false,
            last_display_change_time: 0.0,
            was_focused: true,
            error: None,
            fit: true,
            zoom: 1.0,
            manga_mode: false,
            manga_shift: false,
            rotations: std::collections::HashMap::new(),
            open_from_end: false,
            is_fullscreen: false,
            is_borderless: false,
        };

        if let Some(path) = initial_path {
            app.open_path(path, &cc.egui_ctx);
        }

        app
    }

    pub fn apply_sorting(&mut self) {
        if self.entries_meta.is_empty() {
            return;
        }
        let current_name = self.entries.get(self.current).cloned();

        let mode = self.config.sort_mode;
        let order = self.config.sort_order;
        let natural = self.config.sort_natural;

        self.entries_meta.sort_by(|a, b| {
            let res = match mode {
                crate::config::SortMode::Name => {
                    if natural {
                        crate::archive::natord(&a.name, &b.name)
                    } else {
                        a.name.cmp(&b.name)
                    }
                }
                crate::config::SortMode::Mtime => a.mtime.cmp(&b.mtime),
                crate::config::SortMode::Size => a.size.cmp(&b.size),
            };
            if order == crate::config::SortOrder::Descending {
                res.reverse()
            } else {
                res
            }
        });

        self.entries = self
            .entries_meta
            .iter()
            .map(|e| e.name.clone())
            .collect();

        // ソート後に現在表示していたファイルの位置を特定し直す
        if let Some(name) = current_name {
            if let Some(pos) = self.entries.iter().position(|n| n == &name) {
                self.current = pos;
                self.target_index = pos;
            }
        }
        self.schedule_prefetch();
    }

    // ── アーカイブを開く ──────────────────────────────────────────────────
    pub fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        // 全キャッシュ・ペンディングをクリア
        self.cache.clear();
        self.cache_lru.clear();
        self.pending.clear();
        self.error = None;
        self.current = 0;
        self.target_index = 0;
        self.is_loading_archive = false;
        self.rotations.clear();

        self.is_listing = true;
        let (archive_path, start_name) = if path.is_file() && crate::archive::is_image_ext(&path.to_string_lossy())
        {
            let name = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            let dir = path.parent().unwrap().to_path_buf();
            (dir, Some(name))
        } else {
            (path, None)
        };

        let tx = self.list_tx.clone();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let result = match crate::archive::list_images(&archive_path) {
                Ok(entries) if entries.is_empty() => ListResult {
                    path: archive_path,
                    entries,
                    start_name,
                    go_last: false,
                    error: Some("画像ファイルが見つかりません".to_string()),
                },
                Ok(entries) => ListResult {
                    path: archive_path,
                    entries,
                    start_name,
                    go_last: false,
                    error: None,
                },
                Err(e) => ListResult {
                    path: archive_path,
                    entries: vec![],
                    start_name,
                    go_last: false,
                    error: Some(format!("開けませんでした: {e}")),
                },
            };
            let _ = tx.send(result);
            ctx_clone.request_repaint();
        });
    }

    // ── 回転変更（回転が変わったエントリのキャッシュを無効化） ──────────────
    pub fn invalidate_cache_for(&mut self, index: usize) {
        if let Some(key) = self.cache_key(index) {
            self.cache.remove(&key);
            self.cache_lru.retain(|k| k != &key);
            self.pending.remove(&key);
        }
    }

    pub fn rotate_current(&mut self, cw: bool, ctx: &egui::Context) {
        let indices: Vec<usize> = if self.manga_mode {
            vec![self.current, self.current + 1]
        } else {
            vec![self.current]
        };
        for idx in indices {
            if let Some(name) = self.entries.get(idx).cloned() {
                let rot = self
                    .rotations
                    .get(&name)
                    .copied()
                    .unwrap_or(Rotation::R0);
                self.rotations
                    .insert(name, if cw { rot.cw() } else { rot.ccw() });
                self.invalidate_cache_for(idx);
            }
        }
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    pub fn open_external(&self) {
        let Some(path) = &self.archive_path else { return };
        if self.entries.is_empty() {
            return;
        }

        let entry = &self.entries[self.current];
        let combined = if path.is_dir() {
            path.join(entry).to_string_lossy().to_string()
        } else {
            let base = path.to_string_lossy();
            format!(
                "{}\\{}",
                base.trim_end_matches(|c| c == '\\' || c == '/'),
                entry.trim_start_matches(|c| c == '\\' || c == '/')
            )
        };

        let target_str = combined
            .replace('/', "\\")
            .trim()
            .trim_end_matches('\\')
            .to_string();

        if !self.config.external_app.is_empty() {
            let mut cmd = std::process::Command::new(&self.config.external_app);
            if self.config.external_args.is_empty() {
                cmd.arg(&target_str);
            } else {
                for arg in &self.config.external_args {
                    cmd.arg(arg.replace("%P", &target_str));
                }
            }
            let _ = cmd.spawn();
        }
    }

    pub fn save_config(&self) {
        if let Some(ref path) = self.config_path {
            if let Ok(toml_str) = toml::to_string_pretty(&self.config) {
                let _ = std::fs::write(path, toml_str);
            }
        }
    }

    pub fn process_signals(&mut self, ctx: &egui::Context) {
        // アーカイブリスト取得の完了
        while let Ok(res) = self.list_rx.try_recv() {
            self.is_listing = false;
            self.archive_path = Some(res.path);
            if let Some(err) = res.error {
                self.error = Some(err);
                self.entries.clear();
                self.entries_meta.clear();
                continue;
            }
            self.entries_meta = res.entries;
            self.entries = self
                .entries_meta
                .iter()
                .map(|e| e.name.clone())
                .collect();
            self.apply_sorting();

            if res.go_last {
                let last_idx = self.entries.len().saturating_sub(1);
                self.current = if self.manga_mode && last_idx > 0 {
                    let is_pair_start = if self.manga_shift {
                        last_idx % 2 == 0
                    } else {
                        last_idx % 2 != 0
                    };
                    if is_pair_start {
                        last_idx
                    } else {
                        last_idx.saturating_sub(1)
                    }
                } else {
                    last_idx
                };
            } else if let Some(ref name) = res.start_name {
                self.current = self
                    .entries
                    .iter()
                    .position(|n| {
                        std::path::Path::new(n)
                            .file_name()
                            .map(|f| f.to_string_lossy().as_ref() == name.as_str())
                            .unwrap_or(false)
                    })
                    .unwrap_or(0);
            } else {
                self.current = 0;
            }

            self.target_index = self.current;
            self.is_loading_archive = true;
            self.last_display_change_time = ctx.input(|i| i.time);
            self.schedule_prefetch();
        }

        // 外部プロセスからのパス転送
        while let Ok(path) = self.path_rx.try_recv() {
            self.open_path(path, ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // バックグラウンドロード結果の回収
        self.collect_results(ctx);

        // ドロップされたファイル
        let dropped: Option<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.as_ref().cloned()));
        if let Some(path) = dropped {
            self.open_path(path, ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }
}
