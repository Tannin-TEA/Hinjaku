fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico"); // プロジェクトルートに置いたアイコンファイルを参照
        res.compile().unwrap();
    }
}