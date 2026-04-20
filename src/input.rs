use eframe::egui;
use crate::config::Config;

pub struct KeyboardState {
    pub prev_page: bool,
    pub next_page: bool,
    pub prev_page_single: bool,
    pub next_page_single: bool,
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
    pub zoom_reset: bool,
    pub toggle_manga: bool,
    pub rcw: bool,
    pub rccw: bool,
    pub prev_dir: bool,
    pub next_dir: bool,
    pub sort_settings: bool,
    pub first_page: bool,
    pub last_page: bool,
    pub bs: bool,
    pub open_key_config: bool,
    pub open_external_1: bool,
    pub open_external_2: bool,
    pub open_external_3: bool,
    pub open_external_4: bool,
    pub open_external_5: bool,
    pub toggle_linear: bool,
    pub toggle_rtl: bool,
    pub quit: bool,
    pub toggle_bg: bool,
    pub toggle_debug: bool,
    pub toggle_limiter: bool,
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
            prev_page_single: check("PrevPageSingle"),
            next_page_single: check("NextPageSingle"),
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
            zoom_reset: check("ZoomReset"),
            toggle_manga: check("ToggleManga"),
            rcw: check("RotateCW"),
            rccw: check("RotateCCW"),
            prev_dir: check("PrevDir"),
            next_dir: check("NextDir"),
            sort_settings: check("SortSettings"),
            first_page: check("FirstPage"),
            last_page: check("LastPage"),
            bs: check("RevealExplorer"),
            open_key_config: check("OpenKeyConfig"),
            open_external_1: check("OpenExternal1"),
            open_external_2: check("OpenExternal2"),
            open_external_3: check("OpenExternal3"),
            open_external_4: check("OpenExternal4"),
            open_external_5: check("OpenExternal5"),
            toggle_linear: check("ToggleLinear"),
            toggle_rtl: check("ToggleMangaRtl"),
            quit: check("Quit"),
            toggle_bg: check("ToggleBg"),
            toggle_debug: check("ToggleDebug"),
            toggle_limiter: check("ToggleLimiter"),
        }
    })
}

fn is_pressed(i: &egui::InputState, s: &str) -> bool {
    // "[@]" のような形式なら、文字入力イベント(Text)として判定する
    if s.starts_with('[') && s.ends_with(']') && s.len() > 2 {
        let target = &s[1..s.len() - 1];
        return i.events.iter().any(|e| {
            if let egui::Event::Text(t) = e { t == target } else { false }
        });
    }

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

/// 現在押されているキーの組み合わせを検出し、文字列（例: "Ctrl+A"）として返す
pub fn detect_key_combination(ctx: &egui::Context) -> Option<String> {
    ctx.input(|i| {
        for event in &i.events {
            match event {
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    if let Some(name) = key_to_str(*key) {
                        let mut combo = String::new();
                        if modifiers.ctrl { combo.push_str("Ctrl+"); }
                        if modifiers.alt { combo.push_str("Alt+"); }
                        if modifiers.shift { combo.push_str("Shift+"); }
                        combo.push_str(name);
                        return Some(combo);
                    }
                }
                egui::Event::Text(t) => {
                    // egui::Key に定義されていない記号キーなどが押された場合、
                    // 文字入力イベントを [@ ] のような形式でキャプチャする。
                    if let Some(c) = t.chars().next() {
                        if t.len() == 1 && !c.is_alphanumeric() && !c.is_whitespace() {
                            return Some(format!("[{}]", t));
                        }
                    }
                }
                _ => {}
            }
        }
        None
    })
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
        "f1" => Some(F1), "f2" => Some(F2), "f3" => Some(F3), "f4" => Some(F4), "f5" => Some(F5),
        "f6" => Some(F6), "f7" => Some(F7), "f8" => Some(F8), "f9" => Some(F9), "f10" => Some(F10),
        "f11" => Some(F11), "f12" => Some(F12),
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

/// egui::Key を設定保存用の文字列に変換する
fn key_to_str(key: egui::Key) -> Option<&'static str> {
    use egui::Key::*;
    match key {
        ArrowLeft => Some("ArrowLeft"),
        ArrowRight => Some("ArrowRight"),
        ArrowUp => Some("ArrowUp"),
        ArrowDown => Some("ArrowDown"),
        Enter => Some("Enter"),
        Escape => Some("Escape"),
        Space => Some("Space"),
        Backspace => Some("Backspace"),
        Tab => Some("Tab"),
        Home => Some("Home"),
        End => Some("End"),
        PageUp => Some("PageUp"),
        PageDown => Some("PageDown"),
        Plus => Some("Plus"),
        Equals => Some("Equals"),
        Minus => Some("Minus"),
        F1 => Some("F1"), F2 => Some("F2"), F3 => Some("F3"), F4 => Some("F4"), F5 => Some("F5"),
        F6 => Some("F6"), F7 => Some("F7"), F8 => Some("F8"), F9 => Some("F9"), F10 => Some("F10"),
        F11 => Some("F11"), F12 => Some("F12"),
        A => Some("A"), B => Some("B"), C => Some("C"), D => Some("D"), E => Some("E"), F => Some("F"),
        G => Some("G"), H => Some("H"), I => Some("I"), J => Some("J"), K => Some("K"), L => Some("L"),
        M => Some("M"), N => Some("N"), O => Some("O"), P => Some("P"), Q => Some("Q"), R => Some("R"),
        S => Some("S"), T => Some("T"), U => Some("U"), V => Some("V"), W => Some("W"), X => Some("X"),
        Y => Some("Y"), Z => Some("Z"),
        Num0 => Some("0"), Num1 => Some("1"), Num2 => Some("2"), Num3 => Some("3"), Num4 => Some("4"),
        Num5 => Some("5"), Num6 => Some("6"), Num7 => Some("7"), Num8 => Some("8"), Num9 => Some("9"),
        _ => None,
    }
}