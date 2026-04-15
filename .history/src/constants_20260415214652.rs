//! アプリケーション全体で使用する定数定義

/// キャッシュ関連の定数
pub mod cache {
    /// キャッシュの最大数
    pub const CACHE_MAX: usize = 13;
    /// 先読みする前方ページ数
    pub const PREFETCH_AHEAD: usize = 5;
    /// 先読みする後方ページ数
    pub const PREFETCH_BEHIND: usize = 5;
/// ツリーのノードキャッシュ上限
    pub const TREE_NODES_CACHE_LIMIT: usize = 1000;
}

/// UI関連の定数
pub mod ui {
    /// アーカイブ・フォルダを新規に開いた直後のロック時間 (秒)
    pub const FOLDER_NAV_GUARD_DURATION: f64 = 0.5;
    /// ページめくりやマンガモード同期が完了した直後のロック時間 (秒)
    pub const PAGE_NAV_GUARD_DURATION: f64 = 0.05;
    /// トースト通知の表示時間 (秒)
    pub const TOAST_DURATION: f64 = 5.0;
    /// マウスホイールでページをめくる際のしきい値
    pub const WHEEL_NAV_THRESHOLD: f32 = 40.0;
    /// ズーム操作時の倍率ステップ
    pub const ZOOM_STEP: f32 = 1.2;
    /// ズームの最小倍率 (10%)
    pub const MIN_ZOOM: f32 = 0.1;
    /// ズームの最大倍率 (1000%)
    pub const MAX_ZOOM: f32 = 10.0;
    /// マウスホイールでのズーム感度
    pub const WHEEL_ZOOM_SENSITIVITY: f32 = 0.002;
}

/// 読み込み関連の定数
pub mod loading {
    /// アニメーションを試みる最大ファイルサイズ (30MB)
    pub const MAX_ANIM_DECODE_SIZE: usize = 30 * 1024 * 1024;
    /// アニメーションの最小フレーム遅延 (これより短い場合は 100ms に補正)
    pub const MIN_ANIM_FRAME_DELAY_MS: u32 = 20;
    /// アニメーションのデフォルト遅延
    pub const DEFAULT_ANIM_FRAME_DELAY_MS: u32 = 100;
    /// 1メインループあたりにGPUへ転送する最大テクスチャ数
    pub const MAX_TEXTURE_UPLOADS_PER_FRAME: usize = 2;
    /// 現在位置からこれ以上離れたリクエストは破棄する距離
    pub const LOAD_SKIP_DISTANCE_THRESHOLD: isize = 12;
    /// 画像デコード用ワーカースレッド数
    pub const WORKER_THREADS: usize = 4;
    /// 画像ロード待ち時の自動リトライ最大回数
    pub const LOADING_MAX_RETRIES: u8 = 3;
}

/// 画像処理関連の定数
pub mod image {
    /// 画像の最大解像度 (ピクセル数)
    pub const MAX_IMAGE_DIMENSION: u32 = 8192;
    /// 画像の最小解像度 (ピクセル数)
    pub const MIN_IMAGE_DIMENSION: u32 = 1;
}
    /// 自動リトライの間隔 (ミリ秒)
    pub const LOADING_RETRY_DELAY_MS: u64 = 15;
}

/// 描画関連の定数
pub mod painter {
/// 市松模様のタイルサイズ
    pub const CHECKERBOARD_GRID_SIZE: f32 = 16.0;
    /// 市松模様の色1 (暗い)
    pub const CHECKERBOARD_COLOR_1: u32 = 0xFF191919;
    /// 市松模様の色2 (明るい)
    pub const CHECKERBOARD_COLOR_2: u32 = 0xFF282828;
}

/// 画像処理関連の定数
pub mod image {
    /// テクスチャの最大寸法
    pub const MAX_TEX_DIM: u32 = 4096;
}