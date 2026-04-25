use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let icon_path = "icon.ico";

        // アイコンがなければ、ソフト内のロジックに基づいて自動生成する
        if !Path::new(icon_path).exists() {
            generate_h_icon_file(icon_path);
        }

        let mut res = winres::WindowsResource::new();
        res.set_icon(icon_path);
        let _ = res.compile(); // 失敗してもビルド全体を止めない
    }
}

/// integrator.rs の create_h_icon と同じデザインの ICO ファイルを生成する
fn generate_h_icon_file(path: &str) {
    let size = 32usize;
    let mut pixels = vec![0u8; size * size * 4];

    // integrator.rs と全く同じアルゴリズムでドットを打つ
    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) * 4;
            let is_h = ((6..=10).contains(&x) && (5..=26).contains(&y)) ||
                       ((21..=25).contains(&x) && (5..=26).contains(&y)) ||
                       ((14..=17).contains(&y) && x > 10 && x < 21);
            if is_h {
                // BMP/ICO は BGRA順 かつ 下から上へ格納するのが基本だが、
                // 最近の 32bit(RGBA) DIB ならそのまま書ける
                pixels[i]   = 255; // B
                pixels[i+1] = 255; // G
                pixels[i+2] = 255; // R
                pixels[i+3] = 255; // A
            }
        }
    }

    if let Ok(mut f) = File::create(path) {
        // ICO ヘッダー (6 bytes)
        f.write_all(&[0, 0, 1, 0, 1, 0]).unwrap();
        // ICONDIRENTRY (16 bytes)
        f.write_all(&[
            size as u8, size as u8, 0, 0, 1, 0, 32, 0, 
            ((40 + pixels.len()) & 0xFF) as u8, (((40 + pixels.len()) >> 8) & 0xFF) as u8, 0, 0, // データサイズ
            22, 0, 0, 0 // データ開始位置 (Header 6 + Entry 16)
        ]).unwrap();
        // BITMAPINFOHEADER (40 bytes)
        let mut bmih = vec![0u8; 40];
        bmih[0] = 40; // biSize
        bmih[4..8].copy_from_slice(&(size as u32).to_le_bytes()); // biWidth
        bmih[8..12].copy_from_slice(&( (size * 2) as u32).to_le_bytes()); // biHeight (ICOは2倍にする仕様)
        bmih[12..14].copy_from_slice(&1u16.to_le_bytes()); // biPlanes
        bmih[14..16].copy_from_slice(&32u16.to_le_bytes()); // biBitCount
        f.write_all(&bmih).unwrap();
        
        // ピクセルデータ (ICO/BMPは下から上なので反転させて書き込む)
        for y in (0..size).rev() {
            f.write_all(&pixels[y * size * 4 .. (y + 1) * size * 4]).unwrap();
        }
    }
}