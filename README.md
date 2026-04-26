# Hinjaku - 吹けば飛ぶよな~~軽量~~ビューア

Hinjaku は、Windows 環境に特化した、極めて軽量で高速な画像アーカイブビューアです。  
アーカイブ（ZIP / 7z）内の画像を、一時ファイルを作らずに直接ストリーム読み込みすることで、  
低メモリ消費と高速な閲覧を実現しています。

## 免責事項

本ソフトウェアは「現状のまま」提供されます。利用により生じたいかなる損害についても、作者は責任を負いません。

---

## 主な特徴

- **Windows 専用設計**: Windows API や `ShellExecuteW` を活用。UNC パス (`\\?\`) にも対応。
- **アーカイブ直接ストリーム閲覧**: ZIP / 7z 形式に対応。メモリ内ストリームで処理するため展開不要・高速。
- **PDF 閲覧**: pdfium-render によるネイティブ PDF 表示。
- **GIF / アニメーション WebP**: フレーム単位のデコードとタイミング制御でなめらかに再生。
- **AVIF**: Windows Imaging Component (WIC) 経由でネイティブデコード。
- **ゼロ・テンポラリ**: 展開時にディスクへ一時ファイルを一切書き込みません。
- **外部アプリ連携 (ActionKey)**: 表示中のパスを Photoshop 等の外部ツールへ即座に転送可能。最大9つ設定できます。
- **シングルインスタンス IPC**: 名前付きパイプ経由でパスを既存プロセスへ送り、多重起動を防止。
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
| `←` / `→` (または `P` / `N`) | 前のページ / 次のページ |
| `↑` / `↓` | 1枚前 / 1枚後ろ（見開き時の単ページ送り） |
| `Home` / `End` | 最初のページ / 最後のページ |
| `PgUp` / `PgDn` | 前のフォルダ / 次のフォルダへ移動 |
| `+` / `-` | ズームイン / ズームアウト |
| `F` | フィットモード切替（全体 / 幅合わせ / 等倍） |
| `M` / `Space` | マンガモード（見開き表示）の切替 |
| `Y` | 右開き / 左開きの切替 |
| `I` | 補間フィルタ切替（Nearest / Bilinear / Bicubic / Lanczos） |
| `B` | 背景色の切替 |
| `R` / `Ctrl+R` | 右回転 / 左回転 |
| `T` | ディレクトリツリーの表示 / 非表示 |
| `S` | ソート設定ウィンドウの表示 |
| `K` | キーコンフィグウィンドウの表示 |
| `L` | リミッターモードの切替 |
| `BS` (BackSpace) | 現在のファイルをエクスプローラーで表示 |
| `E` | 外部アプリ1で開く |
| `Enter` | ウィンドウ最大化切替（ツリー表示中は決定） |
| `Alt + Enter` | ボーダレスモード切替 |
| `Esc` | 全画面解除 / ツリーを閉じる |
| `Q` / `Ctrl+W` | アプリを終了 |
| `F12` | デバッグ情報の表示切替 |

> キーはすべてキーコンフィグ設定から変更できます（`K` キー）。

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

外部アプリは `[App_1]`〜`[App_9]` セクションで最大9つ設定できます。  
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
| 最大テクスチャサイズ | 動的（`1920px`〜） | モニタ解像度または 1920px の大きい方に自動調整 |
| 画像キャッシュ | 最大 `16枚` | |
| プリフェッチ | 前方 `3枚` / 後方 `2枚` | バックグラウンドで事前読み込み |
| デコードワーカー | `2スレッド` | 並列処理 |
| アニメーション上限 | `200フレーム` | 超過時は1枚目を静止画として表示 |

### マンガモードのインテリジェンス

- 横長画像（見開き）を検出すると自動的に1枚で中央表示します。
- フォルダの1枚目（表紙想定）は常に単ページ表示し、2枚目から見開きを開始します。
- 左右両ページのロードが完了するまで表示を待機し、表示のちらつきを防止します。

---

## 使用ライブラリ

| ライブラリ | バージョン | 用途 |
|:---|:---|:---|
| **eframe / egui** | 0.27 | GUI フレームワーク（wgpu バックエンド） |
| **image** | 0.25 | 画像デコード（JPEG / PNG / GIF / BMP / WebP） |
| **zip** | 2.1 | ZIP アーカイブのストリーム操作 |
| **sevenz-rust** | 0.6 | 7z アーカイブの読み込み |
| **pdfium-render** | 0.8 | PDF のレンダリング |
| **rust-ini** | 0.21 | INI 設定ファイルのパース・保存 |
| **rfd** | 0.14 | ネイティブなファイル選択ダイアログ |
| **anyhow** | 1 | エラー伝播・コンテキスト付与 |
| **thiserror** | 1.0 | カスタムエラー型の定義 |
| **log** | 0.4 | ログファサード |
| **windows** | 0.58 | COM / WIC（Windows Imaging Component）連携 |
| **windows-sys** | 0.59 | Win32 API 連携（ウィンドウ制御・プロセス起動・パイプ） |
| **winres** *(build)* | 0.1 | Windows リソース埋め込み（アイコン・バージョン情報） |

---

## 内部設計

### モジュール責務

| モジュール | 層 | 責務 |
|:---|:---|:---|
| `main.rs` | エントリ | 引数解析 → 設定読込 → eframe 起動の呼び出し順序のみ |
| `startup.rs` | I/O | CLI 引数解析・二重起動防止 (Mutex)・コンソールアタッチ・タイトル生成 |
| `config.rs` | データ+I/O | Config 構造体定義・INI ファイル読み書き |
| `types.rs` | データ | DisplayMode / ViewState の定義 |
| `error.rs` | データ | HinjakuError / Result\<T\> の定義 |
| `constants.rs` | データ | キャッシュ・UI・画像処理の定数 |
| `utils.rs` | コア | パス正規化・拡張子判定・ファイルサイズ整形・自然順ソート |
| `archive.rs` | I/O | ArchiveReader トレイト + DefaultArchiveReader（ZIP / 7z / フォルダ） |
| `pdf_handler.rs` | I/O | PDF エントリの列挙・ページレンダリング |
| `wic.rs` | I/O | Windows Imaging Component ヘルパー（AVIF デコード等） |
| `manager/mod.rs` | コア | 画像キャッシュ・バックグラウンドロード・プリフェッチ・ページ移動・ワーカースレッド管理 |
| `manager/image_proc.rs` | コア | 画像デコード・リサイズ・回転処理（ワーカースレッド側） |
| `nav_tree.rs` | コア | ディレクトリツリーの構築・選択・展開 |
| `integrator.rs` | I/O | 名前付きパイプによる単一インスタンス IPC・フォント mmap ロード |
| `window.rs` | I/O | ウィンドウ位置・サイズ・アイコン・中央配置（Windows API） |
| `shell.rs` | I/O | 外部アプリ起動・エクスプローラー連携 |
| `input.rs` | UI | キー・マウス入力の解析と KeyboardState への変換 |
| `viewer/mod.rs` | UI | eframe::App 実装。App 構造体・状態管理・アクションディスパッチ・update() ループ |
| `viewer/navigation.rs` | UI | ページ移動・フォルダ移動・シーク・ナビゲーションロック |
| `viewer/display.rs` | UI | ズーム・フィット・フィルタ・マンガモード切替 |
| `viewer/window_mgr.rs` | UI | ウィンドウモード（標準 / ボーダレス / 全画面）・最大化管理 |
| `viewer/input_handler.rs` | UI | キー・マウス入力の振り分けと各アクションの実行 |
| `viewer/render.rs` | UI | egui パネル・ダイアログ・オーバーレイの描画 |
| `painter.rs` | UI | 画像描画（ズーム・マンガモード・回転・背景） |
| `toast.rs` | UI | トースト通知の管理・表示 |
| `widgets/mod.rs` | UI | ViewerAction enum・`get_action_label`・各サブモジュールの re-export |
| `widgets/menu.rs` | UI | メインメニューバー（ファイル / 表示 / オプション等） |
| `widgets/toolbar.rs` | UI | ボトムツールバー（ページ番号・ズーム・モード表示） |
| `widgets/sidebar.rs` | UI | ディレクトリツリーサイドバー |
| `widgets/dialogs/mod.rs` | UI | ダイアログ群の re-export hub |
| `widgets/dialogs/settings.rs` | UI | 外部アプリ連携設定ダイアログ |
| `widgets/dialogs/sort.rs` | UI | ソート設定ダイアログ |
| `widgets/dialogs/key_config.rs` | UI | キーコンフィグ設定ダイアログ |
| `widgets/dialogs/debug.rs` | UI | デバッグ情報ダイアログ |
| `widgets/dialogs/about.rs` | UI | About ダイアログ |
| `widgets/dialogs/limiter.rs` | UI | リミッター設定ダイアログ |

### データフロー

```
[起動]
  main()
    → startup::parse_args
    → config::load_config_file      ← INI ファイル読み込み（1回のみ）
    → startup::check_single_instance
    → eframe::run_native → App::new(config, config_path, ...)

[ファイルを開く]
  App::open_path
    → Manager::open_path
        → ArchiveReader::list_images  ← ZIP / 7z / PDF / フォルダ判定
        → Manager::schedule_prefetch  ← バックグラウンドスレッドに投入

[バックグラウンドロード（ワーカースレッド）]
  image_proc::process_load_request
    → ArchiveReader::read_entry → 画像デコード → FrameData → mpsc::Sender

[描画ループ（毎フレーム）]
  input::gather_input                 ← キー・マウス状態を KeyboardState に集約
    → App::handle_input               ← ツリー/ビューア別に振り分け
    → App::handle_action(ViewerAction)← 状態更新
  App::process_manager_update         ← ロード完了テクスチャを受け取る
  App::sync_display_to_target         ← テクスチャ準備完了を確認して current 更新
  App::draw_ui                        ← パネル・ダイアログ・オーバーレイ描画
    → painter::draw_main_area         ← テクスチャ描画
  window::sync_config_with_window     ← ウィンドウ状態を Config に反映
```

### 依存関係

```
main
 ├─ startup      (引数・プロセス管理)
 ├─ config       (設定読み書き)
 ├─ window       (アイコン生成)
 └─ viewer/  [App]
      ├─ manager/
      │    ├─ image_proc  (デコード・リサイズ・回転)
      │    ├─ archive     (ArchiveReader トレイト)
      │    ├─ pdf_handler
      │    └─ utils       (パス・ソート)
      ├─ painter
      ├─ input
      ├─ widgets/
      │    ├─ mod.rs      (ViewerAction enum・get_action_label)
      │    ├─ menu.rs     (メインメニューバー)
      │    ├─ toolbar.rs  (ボトムツールバー)
      │    ├─ sidebar.rs  (ツリーサイドバー)
      │    └─ dialogs/
      │         ├─ settings.rs   (外部アプリ設定)
      │         ├─ sort.rs       (ソート設定)
      │         ├─ key_config.rs (キーコンフィグ)
      │         ├─ debug.rs      (デバッグ情報)
      │         ├─ about.rs      (About)
      │         └─ limiter.rs    (リミッター設定)
      ├─ window           (位置・サイズ同期)
      ├─ shell            (外部アプリ)
      ├─ integrator       (IPC・フォント)
      ├─ toast
      └─ config           (設定保存)

依存の方向: UI 層 → コア層 → データ層（逆方向なし）
```

### 拡張ポイント

| 追加したい機能 | 変更箇所 |
|:---|:---|
| 新しいアーカイブ形式（RAR 等） | `archive.rs` の `ArchiveReader` トレイトに実装を追加 |
| 新しい表示モード | `types.rs` の `DisplayMode` に variant 追加 → `painter.rs` で分岐 |
| 新しいキー操作 | `widgets/mod.rs` の `ViewerAction` に追加 → `viewer/mod.rs::handle_action` で処理 |
| 新しいダイアログ | `widgets/dialogs/` に追加 → `viewer/render.rs::draw_windows` で呼び出し |
| 設定項目の追加 | `config.rs` の `Config` に追加 → `load_config_file` / `save_config_file` に対応行を追加 |

---

## 開発憲法 (Immutable Rules)

1. **Windows Native Only**: Windows 環境に特化し、UNC パスや OS 固有機能を優先する。
2. **INI-Based Config**: 設定は INI 形式を厳守する。JSON / TOML は採用しない。
3. **Zero-Temp Strategy**: ディスクへの一時ファイル作成を厳禁とし、すべてメモリ内で完結させる。
4. **Speed First**: 「ページめくりが速い」「アイドル時の CPU 消費が極小」を最優先とし、多機能化より軽快さを選ぶ。
5. **No-Code Friendly**: 初心者でも理解しやすい具体的な命名と日本語コメントを維持する。

---

## 開発について

このプロジェクトは **AI 任せ開発** で進めています。  
コードの大部分は [Google Gemini](https://gemini.google.com/) および [Claude (Anthropic)](https://claude.ai/) が生成・レビュー・リファクタリングを担当しており、人間はディレクションと動作確認を担当しています。

- **設計・初期実装**: Google Gemini
- **リファクタリング・コードレビュー・機能追加**: Claude (Anthropic)

---

License: MIT / Apache-2.0  
Developed with Rust, egui, Gemini, and Claude.
