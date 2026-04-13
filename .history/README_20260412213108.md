# ArchView — 圧縮ファイル対応 軽量画像ビューア

Rust + egui で作られた、軽量な Windows 向け画像ビューアです。
zip / 7z / rar 内の画像をそのまま閲覧できます。

## 対応フォーマット

| アーカイブ | 状態 |
|-----------|------|
| .zip      | ✅   |
| .7z       | ✅   |
| .rar      | ✅ (要 unrar ランタイム) |
| フォルダ  | ✅   |

画像: JPEG / PNG / GIF / BMP / WebP / TIFF

## ビルド方法

### 1. Rust のインストール

https://rustup.rs/ からインストーラをダウンロードして実行。

```
rustup default stable
```

### 2. RAR サポートに必要なもの（任意）

[unrar](https://www.rarlab.com/rar_add.htm) の DLL が PATH に必要です。
不要な場合は Cargo.toml から `unrar` を削除してください。

### 3. ビルド

```cmd
cd archview
cargo build --release
```

`target\release\archview.exe` が生成されます。

### Windows 向け最適化ビルド

```cmd
cargo build --release --target x86_64-pc-windows-msvc
```

## 操作方法

| 操作 | 機能 |
|------|------|
| ドラッグ＆ドロップ | ファイル・フォルダを開く |
| `←` / `A` / `Backspace` | 前の画像 |
| `→` / `D` / `Space` | 次の画像 |
| `F` | フィット / 等倍 切り替え |
| `+` / `-` | ズーム |
| `Ctrl` + マウスホイール | ズーム |
| 画像左半分クリック | 前の画像 |
| 画像右半分クリック | 次の画像 |

## 軽量化のポイント

- `opt-level = 3`, `lto = true`, `strip = true` でリリースビルド
- egui は GPU アクセラレーション対応（wgpu バックエンド）
- 画像は 1 枚ずつ遅延ロード（メモリ節約）
