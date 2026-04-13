use crate::App;
use crate::types::ListResult;
use std::path::PathBuf;

impl App {
    pub fn go_prev_dir(&mut self, ctx: &egui::Context) {
        if self.is_listing {
            return;
        }
        if ctx.input(|i| i.time) - self.last_display_change_time < 0.1 {
            return;
        }

        let target = self
            .archive_path
            .as_ref()
            .and_then(|p| self.find_adjacent_path_recursive(p, false));

        if let Some(path) = target {
            self.move_to_dir(path, self.open_from_end, ctx);
        }
    }

    pub fn go_next_dir(&mut self, ctx: &egui::Context) {
        if self.is_listing {
            return;
        }
        if ctx.input(|i| i.time) - self.last_display_change_time < 0.1 {
            return;
        }

        let target = self
            .archive_path
            .as_ref()
            .and_then(|p| self.find_adjacent_path_recursive(p, true));

        if let Some(path) = target {
            self.move_to_dir(path, false, ctx);
        }
    }

    /// マンガモード専用：1ページだけ戻る
    pub fn go_single_prev(&mut self, ctx: &egui::Context) {
        if self.is_listing {
            return;
        }
        if ctx.input(|i| i.time) - self.last_display_change_time < 0.05 {
            return;
        }

        if self.entries.is_empty() || self.target_index == 0 {
            return self.go_prev_dir(ctx);
        }
        self.target_index -= 1;
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    /// マンガモード専用：1ページだけ進む
    pub fn go_single_next(&mut self, ctx: &egui::Context) {
        if self.is_listing {
            return;
        }
        if ctx.input(|i| i.time) - self.last_display_change_time < 0.05 {
            return;
        }

        if self.entries.is_empty() || self.target_index + 1 >= self.entries.len() {
            return self.go_next_dir(ctx);
        }
        self.target_index += 1;
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    pub fn go_prev(&mut self, ctx: &egui::Context) {
        if self.is_listing {
            return;
        }
        if ctx.input(|i| i.time) - self.last_display_change_time < 0.05 {
            return;
        }

        if self.entries.is_empty() || self.target_index == 0 {
            return self.go_prev_dir(ctx);
        }

        let step = if self.manga_mode {
            let first_pair_idx = if self.manga_shift { 0 } else { 1 };
            if self.target_index <= first_pair_idx || self.target_index < 2 {
                1
            } else {
                let prev_is_spread = self.is_spread(self.target_index - 1);
                let prev_prev_is_spread = self.is_spread(self.target_index - 2);
                if prev_is_spread || prev_prev_is_spread {
                    1
                } else {
                    2
                }
            }
        } else {
            1
        };
        self.target_index = self.target_index.saturating_sub(step);
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    pub fn go_next(&mut self, ctx: &egui::Context) {
        if self.is_listing {
            return;
        }
        if ctx.input(|i| i.time) - self.last_display_change_time < 0.05 {
            return;
        }

        if self.entries.is_empty() {
            return self.go_next_dir(ctx);
        }

        let step = if self.manga_mode {
            if self.target_index + 1 >= self.entries.len() {
                1
            } else if !self.manga_shift && self.target_index == 0 {
                1
            } else if self.is_spread(self.target_index) || self.is_spread(self.target_index + 1) {
                1
            } else {
                2
            }
        } else {
            1
        };
        if self.target_index + step >= self.entries.len() {
            return self.go_next_dir(ctx);
        }
        self.target_index += step;
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    pub fn go_first(&mut self, ctx: &egui::Context) {
        if self.entries.is_empty() || self.is_loading_archive {
            return;
        }
        self.target_index = 0;
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    pub fn go_last(&mut self, ctx: &egui::Context) {
        if self.entries.is_empty() || self.is_loading_archive {
            return;
        }
        let last = self.entries.len().saturating_sub(1);
        // マンガモード時は見開きが崩れないよう、最後から2枚目を起点にする
        let target = if self.manga_mode && last > 0 && last % 2 == 0 {
            last.saturating_sub(1)
        } else {
            last
        };
        self.target_index = target;
        self.schedule_prefetch();
        ctx.request_repaint();
    }

    /// 指定したパス（フォルダまたはアーカイブ）の中に画像が存在するかチェックする
    pub fn has_images(&self, path: &std::path::Path) -> bool {
        match crate::archive::list_images(path) {
            Ok(entries) => !entries.is_empty(),
            Err(_) => false,
        }
    }

    /// 再帰的に隣接する「画像を含んだ」ディレクトリまたはアーカイブを探す
    pub fn find_adjacent_path_recursive(&self, path: &std::path::Path, forward: bool) -> Option<PathBuf> {
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_default()
                .join(path)
        };
        let parent = abs_path.parent()?;
        let current_name = abs_path
            .file_name()?
            .to_string_lossy()
            .to_lowercase();

        let mut siblings: Vec<PathBuf> = std::fs::read_dir(parent)
            .ok()?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    || matches!(
                        p.extension()
                            .and_then(|e| e.to_str())
                            .map(|s| s.to_lowercase())
                            .as_deref(),
                        Some("zip" | "7z")
                    )
            })
            .collect();

        siblings.sort_by(|a, b| {
            crate::archive::natord(&a.to_string_lossy(), &b.to_string_lossy())
        });

        // 現在の位置を特定（大文字小文字を無視）
        let idx = siblings.iter().position(|p| {
            p.file_name()
                .map(|f| f.to_string_lossy().to_lowercase())
                .as_deref()
                == Some(&current_name)
        })?;

        if forward {
            for i in (idx + 1)..siblings.len() {
                if self.has_images(&siblings[i]) {
                    return Some(siblings[i].clone());
                }
            }
            // この階層に次がなければ親階層へ遡って次を探す
            self.find_adjacent_path_recursive(parent, true)
        } else {
            for i in (0..idx).rev() {
                if self.has_images(&siblings[i]) {
                    return Some(siblings[i].clone());
                }
            }
            // この階層に前がなければ親階層へ遡って前を探す
            self.find_adjacent_path_recursive(parent, false)
        }
    }

    pub fn move_to_dir(&mut self, path: PathBuf, go_last: bool, ctx: &egui::Context) {
        self.cache.clear();
        self.cache_lru.clear();
        self.pending.clear();
        self.error = None;
        self.rotations.clear();
        self.is_listing = true;

        let tx = self.list_tx.clone();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let result = match crate::archive::list_images(&path) {
                Ok(entries) if entries.is_empty() => ListResult {
                    path,
                    entries,
                    start_name: None,
                    go_last,
                    error: Some("画像ファイルが見つかりません".to_string()),
                },
                Ok(entries) => ListResult {
                    path,
                    entries,
                    start_name: None,
                    go_last,
                    error: None,
                },
                Err(e) => ListResult {
                    path,
                    entries: vec![],
                    start_name: None,
                    go_last,
                    error: Some(format!("開けませんでした: {e}")),
                },
            };
            let _ = tx.send(result);
            ctx_clone.request_repaint();
        });
    }
}
