use std::ffi::OsString;
use std::path::{Path, PathBuf};

use thiserror::Error;
use tokio::process::Command;

#[derive(Clone, Debug)]
pub struct MediaOptimizer {
    ffmpeg: PathBuf,
}

impl Default for MediaOptimizer {
    fn default() -> Self {
        Self {
            ffmpeg: PathBuf::from("ffmpeg"),
        }
    }
}

impl MediaOptimizer {
    #[must_use]
    pub fn with_ffmpeg(ffmpeg: PathBuf) -> Self {
        Self { ffmpeg }
    }

    pub async fn optimize(
        &self,
        input: &Path,
        directory: &Path,
    ) -> Result<OptimizedMedia, MediaError> {
        tokio::fs::create_dir_all(directory).await?;
        let stem = input
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or(MediaError::InvalidInput)?;
        let video_path = directory.join(format!("{stem}.webm"));
        let thumbnail_path = directory.join(format!("{stem}.jpg"));
        if tokio::fs::try_exists(&video_path).await?
            && tokio::fs::try_exists(&thumbnail_path).await?
        {
            return Ok(OptimizedMedia {
                video_path,
                thumbnail_path,
            });
        }

        let temporary_video = directory.join(format!("{stem}.tmp.webm"));
        let status = Command::new(&self.ffmpeg)
            .args(video_arguments(input, &temporary_video))
            .status()
            .await?;
        if !status.success() {
            tokio::fs::remove_file(&temporary_video).await.ok();
            return Err(MediaError::Ffmpeg("video optimization"));
        }
        tokio::fs::rename(&temporary_video, &video_path).await?;

        let status = Command::new(&self.ffmpeg)
            .args(thumbnail_arguments(&video_path, &thumbnail_path))
            .status()
            .await?;
        if !status.success() {
            tokio::fs::remove_file(&video_path).await.ok();
            tokio::fs::remove_file(&thumbnail_path).await.ok();
            return Err(MediaError::Ffmpeg("thumbnail generation"));
        }
        Ok(OptimizedMedia {
            video_path,
            thumbnail_path,
        })
    }
}

fn video_arguments(input: &Path, output: &Path) -> Vec<OsString> {
    ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
        .into_iter()
        .map(OsString::from)
        .chain([input.as_os_str().to_owned()])
        .chain(
            [
                "-an",
                "-vf",
                "scale=1920:1080:force_original_aspect_ratio=decrease:force_divisible_by=2,fps=30",
                "-c:v",
                "libvpx-vp9",
                "-deadline",
                "good",
                "-cpu-used",
                "4",
                "-row-mt",
                "1",
                "-b:v",
                "0",
                "-crf",
                "34",
                "-pix_fmt",
                "yuv420p",
                "-f",
                "webm",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .chain([output.as_os_str().to_owned()])
        .collect()
}

fn thumbnail_arguments(input: &Path, output: &Path) -> Vec<OsString> {
    [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-ss",
        "1",
        "-i",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([input.as_os_str().to_owned()])
    .chain(
        ["-frames:v", "1", "-vf", "scale=480:-2", "-q:v", "3"]
            .into_iter()
            .map(OsString::from),
    )
    .chain([output.as_os_str().to_owned()])
    .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptimizedMedia {
    pub video_path: PathBuf,
    pub thumbnail_path: PathBuf,
}

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("media file failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("media input has no usable file name")]
    InvalidInput,
    #[error("FFmpeg failed during {0}")]
    Ffmpeg(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimization_is_silent_bounded_vp9_at_thirty_fps() {
        let arguments = video_arguments(Path::new("input.mp4"), Path::new("output.webm"))
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(arguments.contains(&"-an".into()));
        assert!(arguments.contains(&"libvpx-vp9".into()));
        assert!(arguments.iter().any(|value| value.contains("1920:1080")));
        assert!(arguments.iter().any(|value| value.contains("fps=30")));
    }
}
