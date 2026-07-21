use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use reqwest::header::CONTENT_LENGTH;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use url::Url;

const USER_AGENT: &str = "Motion/0.1 (https://github.com/ashish-509/wallpapers)";

pub struct DownloadClient {
    client: reqwest::Client,
    max_bytes: u64,
}

impl DownloadClient {
    pub fn new(max_bytes: u64) -> Result<Self, DownloadError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_mins(5))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() > 3 {
                    attempt.error("too many media redirects")
                } else if !is_trusted_url(attempt.url()) {
                    attempt.error("media redirect left upload.wikimedia.org")
                } else {
                    attempt.follow()
                }
            }))
            .build()?;
        Ok(Self { client, max_bytes })
    }

    pub async fn download(
        &self,
        remote_url: &str,
        directory: &Path,
    ) -> Result<DownloadedFile, DownloadError> {
        validate_url(remote_url)?;
        tokio::fs::create_dir_all(directory).await?;

        let request_id = format!("{:x}", Sha256::digest(remote_url.as_bytes()));
        let partial_path = directory.join(format!("{request_id}.part"));
        let response = self
            .client
            .get(remote_url)
            .send()
            .await?
            .error_for_status()?;
        if !is_trusted_url(response.url()) {
            return Err(DownloadError::InvalidUrl);
        }
        if response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|bytes| bytes > self.max_bytes)
        {
            return Err(DownloadError::TooLarge(self.max_bytes));
        }

        let mut file = tokio::fs::File::create(&partial_path).await?;
        let mut stream = response.bytes_stream();
        let mut digest = Sha256::new();
        let mut bytes = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            bytes = bytes.saturating_add(chunk.len() as u64);
            if bytes > self.max_bytes {
                drop(file);
                tokio::fs::remove_file(&partial_path).await.ok();
                return Err(DownloadError::TooLarge(self.max_bytes));
            }
            digest.update(&chunk);
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        file.sync_all().await?;
        drop(file);

        let digest = format!("{:x}", digest.finalize());
        let path = directory.join(format!("{digest}.source"));
        if tokio::fs::try_exists(&path).await? {
            tokio::fs::remove_file(&partial_path).await?;
        } else {
            tokio::fs::rename(&partial_path, &path).await?;
        }
        Ok(DownloadedFile {
            path,
            digest,
            bytes,
        })
    }
}

fn validate_url(value: &str) -> Result<(), DownloadError> {
    let url = Url::parse(value).map_err(|_| DownloadError::InvalidUrl)?;
    if !is_trusted_url(&url) {
        return Err(DownloadError::InvalidUrl);
    }
    Ok(())
}

fn is_trusted_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("upload.wikimedia.org")
        && url.username().is_empty()
        && url.password().is_none()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadedFile {
    pub path: PathBuf,
    pub digest: String,
    pub bytes: u64,
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("download request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("download file failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("download URL is not a trusted Wikimedia HTTPS URL")]
    InvalidUrl,
    #[error("download exceeds the configured {0} byte limit")]
    TooLarge(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_plain_wikimedia_upload_urls() {
        assert!(
            validate_url("https://upload.wikimedia.org/wikipedia/commons/a/a0/rain.webm").is_ok()
        );
        assert!(validate_url("http://upload.wikimedia.org/rain.webm").is_err());
        assert!(validate_url("https://example.com/rain.webm").is_err());
        assert!(validate_url("https://user@upload.wikimedia.org/rain.webm").is_err());
        assert!(!is_trusted_url(
            &Url::parse("https://upload.wikimedia.org.evil.example/rain.webm").unwrap()
        ));
    }
}
