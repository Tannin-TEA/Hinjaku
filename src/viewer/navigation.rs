use eframe::egui;
use std::path::PathBuf;
use crate::{manager, utils};
use crate::types::DisplayMode;
use crate::constants;
use super::App;

impl App {
    pub(super) fn sync_tree_to_current(&mut self) {
        // as_deref() で借用し cleaned を計算した後でボローを解放するため clone 不要
        let Some(cleaned) = self.manager.archive_path.as_deref().map(utils::clean_path) else { return };
        self.manager.tree.expand_to_path(&cleaned);
        self.manager.tree.reveal_path(&cleaned);
        self.manager.tree.selected = Some(cleaned);
    }

    pub(super) fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.error = None;
        let path_str = path.to_string_lossy().into_owned();
        self.config.recent_paths.retain(|p| p != &path_str);
        self.config.recent_paths.insert(0, path_str);
        if self.config.recent_paths.len() > 10 { self.config.recent_paths.pop(); }
        self.save_config();

        if utils::detect_kind(&path) == utils::ArchiveKind::Pdf && self.config.show_pdf_warning {
            self.ui.pdf_warning_open = true;
        }

        self.manager.open_path(path, &self.config);
        if self.manager.tree.nodes.len() > constants::cache::TREE_NODES_CACHE_LIMIT {
            self.manager.tree.clear_metadata_cache();
        }
        self.sync_tree_to_current();
        self.is_loading_archive = self.manager.is_listing;
        ctx.request_repaint();
    }

    pub(super) fn rotate_current(&mut self, cw: bool, ctx: &egui::Context) {
        let indices: Vec<usize> = if self.view.manga_mode {
            vec![self.manager.current, self.manager.current + 1]
        } else {
            vec![self.manager.current]
        };
        for idx in indices {
            if let Some(name) = self.manager.entries.get(idx).cloned() {
                let rot = self.manager.rotations.get(&name).copied().unwrap_or(manager::Rotation::R0);
                self.manager.rotations.insert(name.clone(), if cw { rot.cw() } else { rot.ccw() });
                self.manager.invalidate_cache_for(idx, &name);
            }
        }
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }

    pub(super) fn move_to_dir(&mut self, path: PathBuf, focus_hint: Option<PathBuf>, go_last: bool, ctx: &egui::Context) {
        self.error = None;
        self.manager.move_to_dir(path, focus_hint, go_last, &self.config, self.view.manga_mode, self.view.manga_shift);
        self.sync_tree_to_current();
        self.is_loading_archive = self.manager.is_listing;
        ctx.request_repaint();
    }

    pub(super) fn go_prev_dir(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.navigate_relative_dir(false, ctx);
    }

    pub(super) fn go_next_dir(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.navigate_relative_dir(true, ctx);
    }

    pub(super) fn navigate_relative_dir(&mut self, forward: bool, ctx: &egui::Context) {
        if self.manager.go_relative_dir(forward, &self.config, self.view.manga_mode, self.view.manga_shift) {
            self.sync_tree_to_current();
            self.error = None;
            self.is_loading_archive = true;
            ctx.request_repaint();
        }
    }

    pub(super) fn is_nav_locked(&self, ctx: &egui::Context) -> bool {
        let now = ctx.input(|i| i.time);
        if self.is_loading_archive { return true; }

        // 表示が追いついていない（current != target）間は、次の操作をロックする（非同期スキップ防止）
        if self.manager.current != self.manager.target_index {
            return true;
        }

        if now < self.folder_lock_until {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(self.folder_lock_until - now));
            return true;
        }
        if now < self.page_lock_until {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(self.page_lock_until - now));
            return true;
        }
        false
    }

    pub(super) fn prepare_nav(&mut self) {
        self.reset_nav_locks();
        // Fit/WinFit の時は倍率をデフォルト(1.0)に戻す。等倍(Manual)の時は維持する。
        if self.view.display_mode != DisplayMode::Manual {
            self.view.zoom = 1.0;
        }
    }

    pub(super) fn reset_nav_locks(&mut self) {
        self.folder_lock_until = 0.0;
        self.page_lock_until = 0.0;
    }

    pub(super) fn go_single_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        let (filter, max_dim) = (self.get_effective_filter_mode(), self.get_effective_max_dim(ctx));
        if !self.manager.go_prev(false, false, filter, max_dim) {
            if self.config.limiter_mode && self.config.limiter_stop_at_start { return; }
            self.go_prev_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    pub(super) fn go_single_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        let (filter, max_dim) = (self.get_effective_filter_mode(), self.get_effective_max_dim(ctx));
        if !self.manager.go_next(false, false, filter, max_dim) {
            if self.config.limiter_mode && self.config.limiter_stop_at_end { return; }
            self.go_next_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    pub(super) fn go_prev(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        let (filter, max_dim) = (self.get_effective_filter_mode(), self.get_effective_max_dim(ctx));
        if !self.manager.go_prev(self.view.manga_mode, self.view.manga_shift, filter, max_dim) {
            if self.config.limiter_mode && self.config.limiter_stop_at_start { return; }
            self.go_prev_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    pub(super) fn go_next(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        let (filter, max_dim) = (self.get_effective_filter_mode(), self.get_effective_max_dim(ctx));
        if !self.manager.go_next(self.view.manga_mode, self.view.manga_shift, filter, max_dim) {
            if self.config.limiter_mode && self.config.limiter_stop_at_end { return; }
            self.go_next_dir(ctx);
        } else { ctx.request_repaint(); }
    }

    pub(super) fn go_first(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        self.manager.target_index = 0;
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }

    pub(super) fn go_last(&mut self, ctx: &egui::Context) {
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        let last = self.manager.entries.len().saturating_sub(1);
        self.manager.target_index = if self.view.manga_mode && last > 0 && last.is_multiple_of(2) {
            last.saturating_sub(1)
        } else { last };
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }

    pub(super) fn seek(&mut self, idx: usize, ctx: &egui::Context) {
        // 描写が追いついていない（current != target）間は、新しい移動リクエストを完全に無視する
        if self.is_nav_locked(ctx) { return; }
        self.prepare_nav();
        self.manager.target_index = idx;
        let max_dim = self.get_effective_max_dim(ctx);
        self.manager.schedule_prefetch(self.get_effective_filter_mode(), self.view.manga_mode, max_dim);
        ctx.request_repaint();
    }
}
