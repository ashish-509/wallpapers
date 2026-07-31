use std::collections::HashMap;

use async_trait::async_trait;
use motion_core::{License, RemoteWallpaper, Source};
use scraper::Html;
use serde::Deserialize;

use super::{SourceConnector, SourceError};

const ENDPOINT: &str = "https://commons.wikimedia.org/w/api.php";
const USER_AGENT: &str = "Motion/0.1 (https://github.com/ashish-509/wallpapers)";

pub struct WikimediaCommons {
    client: reqwest::Client,
}

impl WikimediaCommons {
    pub fn new() -> Result<Self, SourceError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .gzip(true)
            .timeout(std::time::Duration::from_secs(20))
            .build()?;
        Ok(Self { client })
    }

    fn parse(response: ApiResponse) -> Vec<RemoteWallpaper> {
        response
            .query
            .map_or_else(Vec::new, |query| query.pages)
            .into_iter()
            .filter_map(parse_page)
            .collect()
    }
}

#[async_trait]
impl SourceConnector for WikimediaCommons {
    async fn search(&self, query: &str, limit: u8) -> Result<Vec<RemoteWallpaper>, SourceError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let search = format!("{query} filetype:video");
        let limit = limit.clamp(1, 20).to_string();
        let parameters = [
            ("action", "query"),
            ("format", "json"),
            ("formatversion", "2"),
            ("generator", "search"),
            ("gsrsearch", search.as_str()),
            ("gsrnamespace", "6"),
            ("gsrlimit", limit.as_str()),
            ("prop", "imageinfo"),
            ("iiprop", "url|mime|extmetadata"),
            ("iiurlwidth", "480"),
            (
                "iiextmetadatafilter",
                "ObjectName|Artist|LicenseShortName|LicenseUrl|Categories|Restrictions",
            ),
            ("maxlag", "5"),
        ];
        let response = self
            .client
            .get(ENDPOINT)
            .query(&parameters)
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse>()
            .await?;
        Ok(Self::parse(response))
    }
}

fn parse_page(page: ApiPage) -> Option<RemoteWallpaper> {
    let info = page.imageinfo.into_iter().next()?;
    if !info.mime.starts_with("video/") {
        return None;
    }

    let metadata = info.extmetadata;
    if metadata
        .get("Restrictions")
        .is_some_and(|value| !value.value.trim().is_empty())
    {
        return None;
    }
    let license_name = metadata.get("LicenseShortName")?.text();
    let license_url = metadata
        .get("LicenseUrl")
        .map_or_else(String::new, |value| value.value.clone());
    let license = License::from_commons(license_name, license_url).ok()?;
    let title = metadata
        .get("ObjectName")
        .map_or_else(|| clean_title(&page.title), MetadataValue::text);
    let author = metadata.get("Artist")?.text();
    if author.is_empty() {
        return None;
    }
    let categories = metadata.get("Categories").map_or_else(Vec::new, |value| {
        value
            .value
            .split('|')
            .map(str::trim)
            .filter(|category| !category.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    });

    Some(RemoteWallpaper {
        source: Source::WikimediaCommons,
        source_id: page.pageid.to_string(),
        title,
        author,
        description_url: info.descriptionurl,
        download_url: info.url,
        thumbnail_url: info.thumburl,
        mime_type: info.mime,
        license,
        categories,
    })
}

fn clean_title(title: &str) -> String {
    title
        .strip_prefix("File:")
        .unwrap_or(title)
        .rsplit_once('.')
        .map_or(title, |(name, _)| name)
        .replace('_', " ")
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    query: Option<ApiQuery>,
}

#[derive(Debug, Deserialize)]
struct ApiQuery {
    pages: Vec<ApiPage>,
}

#[derive(Debug, Deserialize)]
struct ApiPage {
    pageid: u64,
    title: String,
    #[serde(default)]
    imageinfo: Vec<ImageInfo>,
}

#[derive(Debug, Deserialize)]
struct ImageInfo {
    url: String,
    descriptionurl: String,
    thumburl: String,
    mime: String,
    #[serde(default)]
    extmetadata: HashMap<String, MetadataValue>,
}

#[derive(Debug, Deserialize)]
struct MetadataValue {
    value: String,
}

impl MetadataValue {
    fn text(&self) -> String {
        let html = Html::parse_fragment(&self.value);
        html.root_element()
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESPONSE: &str = r#"{
      "query": {"pages": [
        {"pageid": 42, "title": "File:Rain.webm", "imageinfo": [{
          "url": "https://upload.wikimedia.org/rain.webm",
          "descriptionurl": "https://commons.wikimedia.org/wiki/File:Rain.webm",
          "thumburl": "https://upload.wikimedia.org/rain.jpg",
          "mime": "video/webm",
          "extmetadata": {
            "ObjectName": {"value": "Summer rain"},
            "Artist": {"value": "<a href=\"/wiki/User:Example\">Example Author</a>"},
            "LicenseShortName": {"value": "CC BY-SA 4.0"},
            "LicenseUrl": {"value": "https://creativecommons.org/licenses/by-sa/4.0"},
            "Categories": {"value": "Rain|Nature"},
            "Restrictions": {"value": ""}
          }
        }]},
        {"pageid": 43, "title": "File:Locked.mp4", "imageinfo": [{
          "url": "https://example.com/locked.mp4",
          "descriptionurl": "https://example.com/locked",
          "thumburl": "https://example.com/locked.jpg",
          "mime": "video/mp4",
          "extmetadata": {
            "Artist": {"value": "Owner"},
            "LicenseShortName": {"value": "All rights reserved"},
            "LicenseUrl": {"value": "https://example.com/terms"}
          }
        }]},
        {"pageid": 44, "title": "File:Black holes.webm", "imageinfo": [{
          "url": "https://upload.wikimedia.org/black_holes.webm",
          "descriptionurl": "https://commons.wikimedia.org/wiki/File:Black_holes.webm",
          "thumburl": "https://upload.wikimedia.org/black_holes.jpg",
          "mime": "video/webm",
          "extmetadata": {
            "Artist": {"value": "NASA's Goddard Space Flight Center"},
            "LicenseShortName": {"value": "Public domain"}
          }
        }]}
      ]}
    }"#;

    #[test]
    fn parses_attributed_free_video_and_rejects_non_free_results() {
        let response = serde_json::from_str::<ApiResponse>(RESPONSE).unwrap();
        let wallpapers = WikimediaCommons::parse(response);
        assert_eq!(wallpapers.len(), 2);
        assert_eq!(wallpapers[0].title, "Summer rain");
        assert_eq!(wallpapers[0].author, "Example Author");
        assert_eq!(wallpapers[0].categories, vec!["Rain", "Nature"]);
    }

    #[test]
    fn accepts_public_domain_video_without_license_url() {
        let response = serde_json::from_str::<ApiResponse>(RESPONSE).unwrap();
        let wallpapers = WikimediaCommons::parse(response);
        let public_domain = &wallpapers[1];
        assert_eq!(public_domain.title, "Black holes");
        assert_eq!(public_domain.license.name, "Public domain");
        assert_eq!(public_domain.license.url, "");
        assert!(!public_domain.license.attribution_required);
    }
}
