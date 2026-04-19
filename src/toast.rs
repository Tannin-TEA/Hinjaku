//! トースト通知 - 画面右下にふわっと出てすぐ消える通知UI

use eframe::egui::{self, Color32, RichText};
use crate::constants::ui::TOAST_DURATION;

/// トースト通知の種類
#[derive(Clone, PartialEq)]
pub enum ToastKind {
    Info,
    Warning,
    Error,
}

/// 1件のトースト通知
struct Toast {
    message: String,
    kind: ToastKind,
    /// 消滅予定時刻（egui の time）
    expires_at: f64,
}

impl Toast {
    /// 残り時間の割合（0.0=消滅直前 〜 1.0=表示直後）
    fn alpha(&self, now: f64) -> f32 {
        let remaining = (self.expires_at - now) as f32;
        // 最後の0.5秒でフェードアウト
        (remaining / 0.5).clamp(0.0, 1.0)
    }

    fn bg_color(&self) -> Color32 {
        match self.kind {
            ToastKind::Info    => Color32::from_rgb(40, 40, 60),
            ToastKind::Warning => Color32::from_rgb(80, 60, 20),
            ToastKind::Error   => Color32::from_rgb(80, 20, 20),
        }
    }

    fn accent_color(&self) -> Color32 {
        match self.kind {
            ToastKind::Info    => Color32::from_rgb(100, 160, 255),
            ToastKind::Warning => Color32::from_rgb(255, 200, 60),
            ToastKind::Error   => Color32::from_rgb(255, 80, 80),
        }
    }

    fn icon(&self) -> &str {
        match self.kind {
            ToastKind::Info    => "ℹ",
            ToastKind::Warning => "⚠",
            ToastKind::Error   => "✕",
        }
    }
}

/// トースト通知のマネージャー
pub struct ToastManager {
    toasts: Vec<Toast>,
}

impl ToastManager {
    pub fn new() -> Self {
        Self { toasts: Vec::new() }
    }

    /// Info トーストを追加（最もよく使うショートハンド）
    pub fn add(&mut self, message: impl Into<String>, ctx: &egui::Context) {
        self.add_kind(message, ToastKind::Info, ctx);
    }

    /// 警告トーストを追加
    pub fn warn(&mut self, message: impl Into<String>, ctx: &egui::Context) {
        self.add_kind(message, ToastKind::Warning, ctx);
    }

    /// エラートーストを追加
    pub fn error(&mut self, message: impl Into<String>, ctx: &egui::Context) {
        self.add_kind(message, ToastKind::Error, ctx);
    }

    fn add_kind(&mut self, message: impl Into<String>, kind: ToastKind, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let msg = message.into();

        // 同じメッセージが既に表示中なら時刻をリセットするだけ
        if let Some(existing) = self.toasts.iter_mut().find(|t| t.message == msg) {
            existing.expires_at = now + TOAST_DURATION;
            return;
        }

        self.toasts.push(Toast {
            message: msg,
            kind,
            expires_at: now + TOAST_DURATION,
        });

        // 最大5件まで（古いものから削除）
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }

        ctx.request_repaint();
    }

    /// 毎フレーム呼ぶ。期限切れを削除しつつ画面右下に描画する
    pub fn draw(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);

        // 期限切れ削除
        self.toasts.retain(|t| t.expires_at > now);
        if self.toasts.is_empty() { return; }

        // アニメーション中のものがあれば再描画をリクエスト
        let needs_repaint = self.toasts.iter().any(|t| (t.expires_at - now) < 0.5);
        if needs_repaint { ctx.request_repaint(); }

        // 画面右下に積み上げ表示
        egui::Area::new(egui::Id::new("toast_area"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for toast in self.toasts.iter().rev() {
                        let alpha = toast.alpha(now);
                        let bg   = color_with_alpha(toast.bg_color(),     (200.0 * alpha) as u8);
                        let acc  = color_with_alpha(toast.accent_color(),  (255.0 * alpha) as u8);
                        let txt  = color_with_alpha(Color32::WHITE,        (230.0 * alpha) as u8);

                        egui::Frame::none()
                            .fill(bg)
                            .rounding(6.0)
                            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                            .stroke(egui::Stroke::new(1.0, acc))
                            .show(ui, |ui| {
                                ui.set_max_width(360.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(toast.icon()).color(acc).size(16.0));
                                    ui.label(RichText::new(&toast.message).color(txt).size(13.0));
                                });
                            });
                    }
                });
            });
    }
}

/// Color32 のアルファ値だけ差し替えるヘルパー
fn color_with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}