# Hinjaku - 吹けば飛ぶよな~~軽量~~ビューア

Hinjaku は、Windows 環境に特化した、極めて軽量で高速な画像アーカイブビューアです。  
アーカイブ（ZIP/7z）内の画像を、一時ファイルを作らずに直接ストリーム読み込みすることで、低メモリ消費と高速な閲覧を実現しています。

## 免責事項

本ソフトウェアは「現状のまま」提供されます。利用により生じたいかなる損害についても、作者は責任を負いません。

---

## 主な特徴

- **Windows 専用設計**: Windows API や `ShellExecuteW` を活用。UNC パス (`\\?\`) にも対応。
- **アーカイブ直接ストリーム閲覧**: ZIP / 7z 形式に対応。メモリ内ストリームで処理するため極めて高速。
- **PDF 閲覧**: pdfium-render によるネイティブ PDF 表示に対応。
- **ゼロ・テンポラリ**: 展開時にディスクへ一時ファイルを一切書き込みません。
- **外部アプリ連携 (ActionKey)**: 表示中のパスを Photoshop 等の外部ツールへ即座に転送可能。
- **ポータブル設計**: 設定は実行ファイルと同階層の `config.ini` で完結します。

---

## コマンドラインオプション

```bash
hinjaku.exe [パス] [-c 設定名] [--debug]
```

| オプション | 説明 |
|:---|:---|
| `パス` | 起動時に開く画像、フォルダ、またはアーカイブのパス |
| `-c`, `--config` [設定名] | 使用する INI プロファイル名を指定（例: `-c sub` → `sub.ini`）。未指定時は `config.ini` |
| `-d`, `--debug` | コンソールを開き、メモリ使用量やキャッシュ統計のデバッグログを出力 |

### 外部アプリ連携の変数

設定ファイル（config.ini）の `Args` 項目で使用できます。

| 変数 | 説明 |
|:---|:---|
| `%P` (または `%F`) | アーカイブ内のエントリまで含んだ**仮想フルパス** |
| `%A` (または `%D`) | 対象の**物理的な実在パス**。通常時は画像、アーカイブ閲覧時は書庫本体 |

---

## 主なキー操作

| キー | 操作 |
|:---|:---|
| `←` / `→` (P / N) | 前のページ / 次のページ |
| `↑` / `↓` | 1枚前 / 1枚後ろ（見開き時の単ページ送り） |
| `Home` / `End` | 最初のページ / 最後のページ |
| `PgUp` / `PgDn` | 前のフォルダ / 次のフォルダへ移動 |
| `+` / `-` | ズームイン / ズームアウト |
| `F` | フィットモード切替（全体 / 幅合わせ / 等倍） |
| `M` / `Space` | マンガモード（見開き表示）の切替 |
| `Y` | 右開き / 左開きの切替 |
| `I` | 画像補間フィルタ（Nearest / Bilinear / Bicubic / Lanczos）の切替 |
| `B` | 背景色の切替 |
| `R` / `Ctrl+R` | 右回転 / 左回転 |
| `T` | ディレクトリツリーの表示 / 非表示 |
| `S` | ソート設定ウィンドウの表示 |
| `K` | キーコンフィグウィンドウの表示 |
| `L` | リミッターモードの切替 |
| `BS` (BackSpace) | 現在のファイルをエクスプローラーで表示 |
| `E` | 設定した外部アプリ1で開く |
| `Enter` | フルスクリーン切替 |
| `Alt + Enter` | ボーダレス最大化切替 |
| `Esc` | 全画面解除 / ウィンドウを閉じる / ツリーを閉じる |
| `Q` / `Ctrl+W` | アプリを終了 |
| `F12` | デバッグ情報の表示切替 |

---

## 設定項目 (config.ini)

`[Global]` セクションの主な設定キーです。

| キー | 値の例 | 説明 |
|:---|:---|:---|
| `SortMode` | `Name` / `Mtime` / `Size` | ソート基準（ファイル名 / 更新日時 / サイズ） |
| `SortOrder` | `Ascending` / `Descending` | 昇順 / 降順 |
| `SortNatural` | `true` / `false` | 自然順（数値の大きさを考慮）ソートの有効化 |
| `FilterMode` | `Bilinear` / `Nearest` / `Bicubic` / `Lanczos` | 画像補間フィルタ |
| `BackgroundMode` | `Theme` / `Black` / `Gray` / `White` / `Checkerboard` / `Green` | 透明画像の背景 |
| `MangaRtl` | `true` / `false` | 右開き（RTL）モードのデフォルト |
| `AllowMultipleInstances` | `true` / `false` | 多重起動の許可 |
| `AlwaysOnTop` | `true` / `false` | 常に最前面に表示 |
| `LimiterMode` | `true` / `false` | リミッターモード（ページ送り制限）の有効化 |
| `LimiterPageDuration` | `0.05` | リミッター：ページ間の最短待機時間（秒） |
| `LimiterFolderDuration` | `0.2` | リミッター：フォルダ移動の最短待機時間（秒） |
| `Mouse4Action` / `Mouse5Action` | `PrevPage` 等 | マウスサイドボタンのアクション割り当て |

外部アプリは `[App_1]`〜`[App_5]` セクションで最大5つ設定できます。  
キーバインドは `[KeyConfig]` セクションで上書き可能です。

---

## 技術仕様・内部パラメータ

### ナビゲーション・ガード（誤操作防止）

| パラメータ | 値 | 説明 |
|:---|:---|:---|
| フォルダ移動ロック | `0.1秒` | 移動直後の意図しない連打を防止 |
| ページ移動ロック | `0.01秒` | マンガモード等の揃え待ち中のガタつきを防止 |
| ホイールしきい値 | `40.0` | 一定量のスクロール蓄積でページをめくる |

### パフォーマンスと制限

| パラメータ | 値 | 説明 |
|:---|:---|:---|
| 最大テクスチャサイズ | 動的 (`1920px`〜) | モニタ解像度または1920pxの大きい方に合わせて自動調整 |
| 画像キャッシュ | 最大 `16枚` | |
| プリフェッチ | 前方 `3枚` / 後方 `2枚` | バックグラウンドで事前読み込み |
| デコードワーカー | `2スレッド` | 並列処理 |

### マンガモードのインテリジェンス

- 見開き（横長）画像を検出すると、自動的に1枚で中央表示します。
- フォルダの1枚目（表紙想定）は常に1枚で表示し、2枚目から見開きを開始します。
- 左右のページが両方ロードされるまで表示を待機し、表示の瞬きを防止します。

---

## 使用ライブラリ

| ライブラリ | 用途 |
|:---|:---|
| **eframe / egui** | GUI フレームワーク |
| **image** | 画像デコード処理（JPEG / PNG / GIF / BMP / WebP） |
| **zip-rs** | ZIP アーカイブのストリーム操作 |
| **sevenz-rust** | 7z アーカイブの読み込み |
| **pdfium-render** | PDF のレンダリング |
| **rust-ini** | INI 設定ファイルのパース・保存 |
| **windows-sys** | Win32 API 連携（ウィンドウ制御・プロセス起動） |
| **windows** | COM / WIC（Windows Imaging Component）連携 |
| **rfd** | ネイティブなファイル選択ダイアログ |

---

## 内部設計

### モジュール責務

| モジュール | 層 | 責務 |
|:---|:---|:---|
| `main.rs` | エントリ | 引数解析→設定読込→eframe起動の呼び出し順序のみ |
| `startup.rs` | I/O | CLI引数解析・二重起動防止(Mutex)・コンソールアタッチ・タイトル生成 |
| `config.rs` | データ+I/O | Config構造体定義・INIファイル読み書き |
| `types.rs` | データ | DisplayMode / ViewState の定義 |
| `error.rs` | データ | HinjakuError / Result\<T\> の定義 |
| `constants.rs` | データ | キャッシュ・UI・画像処理の定数 |
| `utils.rs` | コア | パス正規化・拡張子判定・ファイルサイズ整形・自然順ソート |
| `archive.rs` | I/O | ArchiveReader トレイト + DefaultArchiveReader (ZIP/7z/フォルダ) |
| `pdf_handler.rs` | I/O | PDF エントリの列挙・ページレンダリング |
| `wic.rs` | I/O | Windows Imaging Component ヘルパー |
| `manager.rs` | コア | 画像キャッシュ・バックグラウンドロード・プリフェッチ・ページ移動 |
| `nav_tree.rs` | コア | ディレクトリツリーの構築・選択・展開 |
| `integrator.rs` | I/O | WM_COPYDATA による単一インスタンス間パス受信・フォント mmap ロード |
| `window.rs` | I/O | ウィンドウ位置・サイズ・アイコン・中央配置 (Windows API) |
| `shell.rs` | I/O | 外部アプリ起動・エクスプローラー連携 |
| `input.rs` | UI | キー・マウス入力の解析と ViewerAction への変換 |
| `viewer.rs` | UI | eframe::App 実装。状態管理・アクション処理・update() ループ |
| `painter.rs` | UI | 画像描画（ズーム・マンガモード・回転・背景） |
| `toast.rs` | UI | トースト通知の管理・表示 |
| `widgets/` | UI | ViewerAction enum・ツールバー・メニュー・サイドバー・ダイアログ |

### データフロー

```
[起動]
  main()
    → startup::parse_args
    → config::load_config_file      ← INIファイル読み込み (1回のみ)
    → startup::check_single_instance
    → eframe::run_native → App::new(config, config_path, ...)

[ファイルを開く]
  App::open_path
    → Manager::open_path
        → ArchiveReader::list_images  ← ZIP/7z/PDF/フォルダ判定
        → Manager::schedule_prefetch  ← バックグラウンドスレッドに投入

[バックグラウンドロード (ワーカースレッド)]
  ArchiveReader::read_entry → image decode → TextureHandle → mpsc::Sender

[描画ループ (毎フレーム)]
  input::capture_key / capture_mouse
    → widgets → ViewerAction
    → App::handle_action → 状態更新
  Manager::poll_results               ← ロード完了テクスチャを受け取る
  painter::draw_main_area             ← テクスチャ描画
  window::sync_config_with_window     ← ウィンドウ状態を Config に反映
```

### 依存関係

```
main
 ├─ startup      (引数・プロセス管理)
 ├─ config       (設定読み書き)
 ├─ window       (アイコン生成)
 └─ viewer  [App]
      ├─ manager
      │    ├─ archive  (ArchiveReader トレイト)
      │    ├─ pdf_handler
      │    └─ utils    (パス・ソート)
      ├─ painter
      ├─ input
      ├─ widgets
      ├─ window        (位置・サイズ同期)
      ├─ shell         (外部アプリ)
      ├─ integrator    (IPC・フォント)
      ├─ toast
      └─ config        (設定保存)

依存の方向: UI層 → コア層 → データ層 (逆方向なし)
```

### 拡張ポイント

| 追加したい機能 | 変更箇所 |
|:---|:---|
| 新しいアーカイブ形式 (RAR等) | `archive.rs` の `ArchiveReader` トレイトに実装を追加 |
| 新しい表示モード | `types.rs` の `DisplayMode` に variant 追加 → `painter.rs` で分岐 |
| 新しいキー操作・マウス操作 | `widgets/mod.rs` の `ViewerAction` に追加 → `viewer.rs::handle_action` で処理 |
| 新しいダイアログ・パネル | `widgets/dialogs.rs` に追加 → `viewer.rs::update()` で呼び出し |
| 設定項目の追加 | `config.rs` の `Config` に追加 → `load_config_file` / `save_config_file` に対応行を追加 |

---

## 開発憲法 (Immutable Rules)

1. **Windows Native Only**: Windows 環境に特化し、UNC パスや OS 固有機能を優先する。
2. **INI-Based Config**: 設定は INI 形式を厳守する。JSON/TOML は採用しない。
3. **Zero-Temp Strategy**: ディスクへの一時ファイル作成を厳禁とし、すべてメモリ内で完結させる。
4. **No-Code Friendly**: 初心者でも理解しやすい具体的な命名と日本語コメントを維持する。

---

## 開発について

このプロジェクトは **AI 任せ開発** で進めています。  
コードの大部分は [Google Gemini](https://gemini.google.com/) および [Claude (Anthropic)](https://claude.ai/) に生成・レビュー・リファクタリングを任せており、人間はディレクションと動作確認を担当しています。

---

License: MIT / Apache-2.0  
Developed with Rust, egui, Gemini, and Claude.
