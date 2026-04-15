use thiserror::Error;

#[derive(Error, Debug)]
pub enum HinjakuError {
    #[error("入出力エラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("アーカイブ処理エラー: {0}")]
    Archive(String),

    #[error("ファイルが見つかりません: {0}")]
    NotFound(String),

    #[error("設定エラー: {0}")]
    Config(String),

    #[error("デコードエラー: {0}")]
    Decode(String),
}

impl HinjakuError {
    pub fn user_message(&self) -> String {
        match self {
            Self::NotFound(_) => "指定されたファイルまたはディレクトリが見つかりません。".to_string(),
            _ => self.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, HinjakuError>;