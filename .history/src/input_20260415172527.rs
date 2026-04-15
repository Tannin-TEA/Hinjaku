use eframe::egui;
use crate::config::Config;

pub struct KeyboardState {
    pub prev_page: bool,
    pub next_page: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub dn: bool,
    pub enter: bool,
    pub fullscreen: bool,
    pub borderless: bool,
    pub esc: bool,
    pub toggle_tree: bool,
    pub toggle_fit: bool,
    pub zoom_in: bool,
    pub zoom_out: bool,
    pub toggle_manga: bool,
    pub rcw: bool,
    pub rccw: bool,
    pub prev_dir: bool,
    pub next_dir: bool,
    pub sort_settings: bool,
    pub first_page: bool,
    pub last_page: bool,
    pub bs: bool,
    pub open_external: bool,
    pub toggle_linear: bool,
    pub alt: bool,
    pub toggle_rtl: bool,
    pub quit: bool,
    pub toggle_bg: bool,
}

pub fn gather_input(ctx: &egui::Context, config: &Config) -> KeyboardState {
    ctx.input(|i| {
        let check = |name: &str| -> bool {
            if let Some(mapping) = config.keys.get(name) {
                mapping.split(',').any(|s| is_pressed(i, s.trim()))
            } else { false }
        };

        KeyboardState {
            prev_page: check("PrevPage"),
            next_page: check("NextPage"),
            left: check("Left"),
            right: check("Right"),
            up: check("Up"),
            dn: check("Down"),
            enter: check("Enter"),
            fullscreen: check("ToggleFullscreen"),
            borderless: check("ToggleBorderless"),
            esc: check("Escape"),
            toggle_tree: check("ToggleTree"),
            toggle_fit: check("ToggleFit"),
            zoom_in: check("ZoomIn"),
            zoom_out: check("ZoomOut"),
            toggle_manga: check("ToggleManga"),
            rcw: check("RotateCW"),
            rccw: check("RotateCCW"),
            prev_dir: check("PrevDir"),
            next_dir: check("NextDir"),
            sort_settings: check("SortSettings"),
            first_page: check("FirstPage"),
            last_page: check("LastPage"),
            bs: check("RevealExplorer"),
            open_external: check("OpenExternal1"),
            toggle_linear: check("ToggleLinear"),
            alt: i.modifiers.alt,
            toggle_rtl: check("ToggleMangaRtl"),
            quit: check("Quit"),
            toggle_bg: check("ToggleBg"),
        }
    })
}

fn is_pressed(i: &egui::InputState, s: &str) -> bool {
    let parts: Vec<&str> = s.split('+').collect();
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    for p in &parts[..parts.len().saturating_sub(1)] {
        match p.to_lowercase().as_str() {
            "ctrl" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            _ => {}
        }
    }
    let key_part = parts.last().unwrap_or(&"");
    if let Some(k) = match_key(key_part) {
        i.key_pressed(k) && 
        i.modifiers.ctrl == ctrl && 
        i.modifiers.alt == alt && 
        i.modifiers.shift == shift
    } else {
        false
    }
}

fn match_key(s: &str) -> Option<egui::Key> {
    use egui::Key::*;
    match s.to_lowercase().as_str() {
        "arrowleft" | "left" => Some(ArrowLeft), "arrowright" | "right" => Some(ArrowRight),
        "arrowup" | "up" => Some(ArrowUp), "arrowdown" | "down" => Some(ArrowDown),
        "enter" => Some(Enter), "escape" | "esc" => Some(Escape), "space" => Some(Space),
        "backspace" | "bs" => Some(Backspace), "tab" => Some(Tab), "home" => Some(Home), "end" => Some(End),
        "pageup" | "pgup" => Some(PageUp), "pagedown" | "pgdn" => Some(PageDown),
        "plus" => Some(Plus), "equals" => Some(Equals), "minus" => Some(Minus),
        "a" => Some(A), "b" => Some(B), "c" => Some(C), "d" => Some(D), "e" => Some(E), "f" => Some(F),
        "g" => Some(G), "h" => Some(H), "i" => Some(I), "j" => Some(J), "k" => Some(K), "l" => Some(L),
        "m" => Some(M), "n" => Some(N), "o" => Some(O), "p" => Some(P), "q" => Some(Q), "r" => Some(R),
        "s" => Some(S), "t" => Some(T), "u" => Some(U), "v" => Some(V), "w" => Some(W), "x" => Some(X),
        "y" => Some(Y), "z" => Some(Z),
        "0" => Some(Num0), "1" => Some(Num1), "2" => Some(Num2), "3" => Some(Num3), "4" => Some(Num4),
        "5" => Some(Num5), "6" => Some(Num6), "7" => Some(Num7), "8" => Some(Num8), "9" => Some(Num9),
        _ => None,
    }
}