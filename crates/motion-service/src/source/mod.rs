mod wikimedia;

use async_trait::async_trait;
use motion_core::RemoteWallpaper;
use thiserror::Error;

pub use wikimedia::WikimediaCommons;

#[async_trait]
pub trait SourceConnector: Send + Sync {
    async fn search(&self, query: &str, limit: u8) -> Result<Vec<RemoteWallpaper>, SourceError>;
}

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("source request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("invalid source configuration: {0}")]
    Configuration(String),
}
