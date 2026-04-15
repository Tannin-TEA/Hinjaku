#[derive(Debug)]
pub enum HinjakuError {
    Io(std::io::Error),
    Archive(String),
    Decode(String),
    Config(String),
    Internal(String),
    NotFound(String),
}

impl From<std::io::Error> for HinjakuError {
    fn from(err: std::io::Error) -> Self {
        HinjakuError::Io(err)
    }
}

impl From<anyhow::Error> for HinjakuError {
    fn from(err: anyhow::Error) -> Self {
        HinjakuError::Internal(err.to_string())
    }
}

impl From<zip::result::ZipError> for HinjakuError {
    fn from(err: zip::result::ZipError) -> Self {
        HinjakuError::Archive(err.to_string())
    }
}

impl HinjakuError {
    /// ユーザーに表示するための簡潔な日本語メッセージ
    pub fn user_message(&self) -> String {
        match self {
            HinjakuError::Io(e) => format!("ファイルにアクセスできません: {}", e.kind()),
            HinjakuError::Archive(msg) => format!("アーカイブの処理に失敗しました: {}", msg),
            HinjakuError::Decode(msg) => format!("画像の表示に失敗しました: {}", msg),
            HinjakuError::Config(msg) => format!("設定の読み書きに失敗しました: {}", msg),
            HinjakuError::NotFound(msg) => format!("見つかりませんでした: {}", msg),
            HinjakuError::Internal(_) => "内部エラーが発生しました".to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, HinjakuError>;