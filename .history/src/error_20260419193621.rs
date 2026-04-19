use thiserror::Error;

#[derive(Error, Debug)]
pub enum HinjakuError {
    #[error("入出力エラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("アーカイブ処理エラー: {0}")]
    Archive(String),

    #[error("ZIPエラー: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("ファイルが見つかりません: {0}")]
    NotFound(String),
}

impl HinjakuError {
    pub fn user_message(&self) -> String {
        match self {
            Self::NotFound(_) => "指定されたファイルまたはディレクトリが見つかりません。".to_string(),
            Self::Archive(msg) => msg.clone(),
            _ => self.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, HinjakuError>;