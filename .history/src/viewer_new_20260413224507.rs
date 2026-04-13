mod app_cache;
mod app_core;
mod app_navigation;
mod app_ui;
mod types;
mod ui_draw;

use crate::config::Config;
use crate::types::{CacheEntry, ListResult, Rotation};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, Sender};
use eframe::egui;

// ── メインアプリ ─────────────────────────────────────────────────────────────
pub struct App {
    pub archive_path: Option<PathBuf>,
    pub entries: Vec<String>,
    pub entries_meta: Vec<crate::archive::ImageEntry>,
    pub current: usize,
    pub target_index: usize,
    pub cache: HashMap<String, CacheEntry>,
    pub cache_lru: VecDeque<String>,
    pub load_tx: Sender<types::LoadRequest>,
    pub load_rx: Receiver<types::LoadResult>,
    pub pending: std::collections::HashSet<String>,
    pub current_idx_shared: Arc<AtomicUsize>,
    pub wheel_accumulator: f32,
    pub path_rx: Receiver<PathBuf>,
    pub list_tx: Sender<ListResult>,
    pub list_rx: Receiver<ListResult>,
    pub config: Config,
    pub show_settings: bool,
    pub show_sort_settings: bool,
    pub sort_focus_idx: usize,
    pub settings_args_tmp: String,
    pub config_path: Option<PathBuf>,
    pub is_listing: bool,
    pub is_loading_archive: bool,
    pub last_display_change_time: f64,
    pub was_focused: bool,
    pub error: Option<String>,
    pub fit: bool,
    pub zoom: f32,
    pub manga_mode: bool,
    pub manga_shift: bool,
    pub rotations: HashMap<String, Rotation>,
    pub open_from_end: bool,
    pub is_fullscreen: bool,
    pub is_borderless: bool,
}

// ── ヘルパー関数 ────────────────────────────────────────────────────────────
pub fn downscale_if_needed(
    img: image::RgbaImage,
    max_dim: u32,
    linear: bool,
) -> image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim {
        return img;
    }
    let scale = (max_dim as f32 / w as f32).min(max_dim as f32 / h as f32);
    let nw = ((w as f32 * scale) as u32).max(1);
    let nh = ((h as f32 * scale) as u32).max(1);
    if linear {
        image::imageops::thumbnail(&img, nw, nh)
    } else {
        image::imageops::resize(
            &img,
            nw,
            nh,
            image::imageops::FilterType::Nearest,
        )
    }
}

pub fn apply_rotation(img: image::RgbaImage, rot: Rotation) -> image::RgbaImage {
    match rot {
        Rotation::R0 => img,
        Rotation::R90 => image::imageops::rotate90(&img),
        Rotation::R180 => image::imageops::rotate180(&img),
        Rotation::R270 => image::imageops::rotate270(&img),
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_focused = ctx.input(|i| i.focused);
        let click_allowed = is_focused && self.was_focused;
        self.process_signals(ctx);

        // ページ同期（目標ページの準備ができていたら表示を更新）
        if self.is_loading_archive || self.current != self.target_index {
            if self.get_texture(self.target_index).is_some() {
                self.current = self.target_index;
                self.is_loading_archive = false;
                self.last_display_change_time = ctx.input(|i| i.time);
            }
        }

        self.handle_input(ctx);

        if self.show_settings { self.ui_settings_window(ctx); }
        if self.show_sort_settings { self.ui_sort_settings_window(ctx); }
        if self.is_listing { self.ui_loading_window(ctx); }

        egui::TopBottomPanel::top("menu").show(ctx, |ui| { self.ui_menu_bar(ui, ctx); });
        egui::TopBottomPanel::bottom("toolbar").show(ctx, |ui| { self.ui_toolbar(ui, ctx); });
        egui::CentralPanel::default().show(ctx, |ui| { self.ui_main_content(ui, ctx, click_allowed); });

        self.was_focused = is_focused;
    }
}

impl App {
    fn handle_input(&mut self, ctx: &egui::Context) {
        let modal_open = self.show_sort_settings || self.show_settings || self.is_listing;
        if modal_open { return; }

        let (left, right, fit_t, zin, zout, manga_t, rcw, rccw, pgup, pgdn, up, dn, p_key, n_key, s_key, home, end, bs_key, e_key, i_key, enter_key, alt_pressed, esc_key) = ctx.input(|i| (
            i.key_pressed(egui::Key::ArrowLeft)  || i.key_pressed(egui::Key::A),
            i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::D),
            i.key_pressed(egui::Key::F),
            i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
            i.key_pressed(egui::Key::Minus),
            i.key_pressed(egui::Key::M) || i.key_pressed(egui::Key::Space),
            i.key_pressed(egui::Key::R) && !i.modifiers.ctrl,
            i.key_pressed(egui::Key::R) &&  i.modifiers.ctrl,
            i.key_pressed(egui::Key::PageUp),
            i.key_pressed(egui::Key::PageDown),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::P),
            i.key_pressed(egui::Key::N),
            i.key_pressed(egui::Key::S),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::End),
            i.key_pressed(egui::Key::Backspace),
            i.key_pressed(egui::Key::E),
            i.key_pressed(egui::Key::I),
            i.key_pressed(egui::Key::Enter),
            i.modifiers.alt,
            i.key_pressed(egui::Key::Escape),
        ));

        if left  || p_key { self.go_prev(ctx); }
        if right || n_key { self.go_next(ctx); }
        if up { if self.manga_mode { self.go_single_prev(ctx); } else { self.go_prev(ctx); } }
        if dn { if self.manga_mode { self.go_single_next(ctx); } else { self.go_next(ctx); } }
        if home { self.go_first(ctx); }
        if end  { self.go_last(ctx); }
        if fit_t  { self.fit = !self.fit; }
        if zin    { self.zoom = (self.zoom * 1.2).min(10.0); self.fit = false; }
        if zout   { self.zoom = (self.zoom / 1.2).max(0.1);  self.fit = false; }
        if rcw    { self.rotate_current(true,  ctx); }
        if rccw   { self.rotate_current(false, ctx); }
        if pgup   { self.go_prev_dir(ctx); }
        if pgdn   { self.go_next_dir(ctx); }
        if manga_t {
            self.manga_mode = !self.manga_mode;
            self.schedule_prefetch();
            ctx.request_repaint();
        }
        if s_key {
            self.show_sort_settings = !self.show_sort_settings;
            if self.show_sort_settings { self.sort_focus_idx = 0; }
        }
        if i_key {
            self.config.linear_filter = !self.config.linear_filter;
            self.cache.clear();
            self.cache_lru.clear();
            self.pending.clear();
            self.save_config();
            self.schedule_prefetch();
        }
        if bs_key {
            if let Some(path) = &self.archive_path {
                let _ = std::process::Command::new("explorer")
                    .arg("/select,")
                    .arg(path)
                    .spawn();
            }
        }
        if e_key { self.open_external(); }
        if enter_key {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            if alt_pressed {
                self.is_borderless = !self.is_borderless;
                self.is_fullscreen = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.is_borderless));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.is_borderless));
            } else {
                self.is_fullscreen = !self.is_fullscreen;
                self.is_borderless = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.is_fullscreen));
            }
        }
        if esc_key {
            if self.is_fullscreen || self.is_borderless {
                self.is_fullscreen = false;
                self.is_borderless = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            }
        }
    }
}
