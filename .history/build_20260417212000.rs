fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        let icon_path = "icon.ico";

        if std::path::Path::new(icon_path).exists() {
            res.set_icon(icon_path);
        } else {
            // ファイルがない場合にビルドを失敗させないよう、cargoの警告として出力する
            println!("cargo:warning=FILE NOT FOUND: {} が見つからないため、アイコンの埋め込みをスキップします。", icon_path);
        }

        let _ = res.compile(); // 失敗してもビルド全体を止めない
    }
}