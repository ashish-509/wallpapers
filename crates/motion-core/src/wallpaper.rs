use serde::{Deserialize, Serialize};

use crate::License;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Source {
    WikimediaCommons,
    LocalImport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemoteWallpaper {
    pub source: Source,
    pub source_id: String,
    pub title: String,
    pub author: String,
    pub description_url: String,
    pub download_url: String,
    pub thumbnail_url: String,
    pub mime_type: String,
    pub license: License,
    pub categories: Vec<String>,
}

impl RemoteWallpaper {
    #[must_use]
    pub fn attribution(&self) -> String {
        format!(
            "{} by {} | {} | {}",
            self.title, self.author, self.license.name, self.description_url
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribution_retains_creator_license_and_source() {
        let wallpaper = RemoteWallpaper {
            source: Source::WikimediaCommons,
            source_id: "42".into(),
            title: "Rain".into(),
            author: "Example Author".into(),
            description_url: "https://commons.wikimedia.org/wiki/File:Rain.webm".into(),
            download_url: "https://upload.wikimedia.org/rain.webm".into(),
            thumbnail_url: "https://upload.wikimedia.org/rain.jpg".into(),
            mime_type: "video/webm".into(),
            license: License::from_commons(
                "CC BY 4.0",
                "https://creativecommons.org/licenses/by/4.0/",
            )
            .unwrap(),
            categories: vec!["rain".into(), "nature".into()],
        };

        assert_eq!(
            wallpaper.attribution(),
            "Rain by Example Author | CC BY 4.0 | https://commons.wikimedia.org/wiki/File:Rain.webm"
        );
    }
}
