use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use futures_util::{StreamExt, stream};
use gtk::{gio, prelude::*};
use motion_core::{RemoteWallpaper, eviction_plan};
use motion_service::{
    Catalog, CatalogItem, DownloadClient, MediaOptimizer, SourceConnector, WikimediaCommons,
};

use crate::paths::AppPaths;

const SETTINGS_ID: &str = "io.github.ashish509.Motion";
const MAX_MEDIA_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_PREVIEW_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug)]
pub struct SearchResult {
    pub wallpaper: RemoteWallpaper,
    pub thumbnail: Option<PathBuf>,
}

#[derive(Debug)]
pub struct PreparedMedia {
    pub video: PathBuf,
    pub thumbnail: PathBuf,
    pub bytes: u64,
}

pub async fn discover(query: String, previews: PathBuf) -> Result<Vec<SearchResult>> {
    let source = WikimediaCommons::new()?;
    let wallpapers = source.search(&query, 16).await?;
    let client = Arc::new(DownloadClient::new(MAX_PREVIEW_BYTES)?);
    let results = stream::iter(wallpapers.into_iter().map(|wallpaper| {
        let client = Arc::clone(&client);
        let previews = previews.clone();
        async move {
            let thumbnail = client
                .download(&wallpaper.thumbnail_url, &previews)
                .await
                .ok()
                .map(|download| download.path);
            SearchResult {
                wallpaper,
                thumbnail,
            }
        }
    }))
    .buffer_unordered(4)
    .collect()
    .await;
    Ok(results)
}

pub async fn prepare(wallpaper: RemoteWallpaper, paths: AppPaths) -> Result<PreparedMedia> {
    let download = DownloadClient::new(MAX_MEDIA_BYTES)?
        .download(&wallpaper.download_url, &paths.downloads)
        .await?;
    let optimized = MediaOptimizer::default()
        .optimize(&download.path, &paths.media)
        .await;
    tokio::fs::remove_file(&download.path).await.ok();
    let optimized = optimized?;
    let bytes = file_size(&optimized.video_path)? + file_size(&optimized.thumbnail_path)?;
    Ok(PreparedMedia {
        video: optimized.video_path,
        thumbnail: optimized.thumbnail_path,
        bytes,
    })
}

pub fn persist_search(
    catalog: &Catalog,
    query: &str,
    results: Vec<SearchResult>,
    fetched_at: u64,
) -> Result<()> {
    for result in results {
        let id = catalog.upsert(&result.wallpaper)?;
        if let Some(thumbnail) = result.thumbnail {
            catalog.record_thumbnail(&id, &thumbnail)?;
        }
    }
    catalog.remember_search(query, fetched_at)?;
    Ok(())
}

pub fn settings() -> Option<gio::Settings> {
    let source = gio::SettingsSchemaSource::default()?;
    let schema = source.lookup(SETTINGS_ID, true)?;
    Some(gio::Settings::new_full(
        &schema,
        gio::SettingsBackend::NONE,
        None,
    ))
}

pub fn apply(
    catalog: &Catalog,
    settings: &gio::Settings,
    item: &CatalogItem,
    used_at: u64,
) -> Result<()> {
    let path = item
        .local_path
        .as_deref()
        .context("download this wallpaper before applying it")?;
    if !path.is_file() {
        return Err(anyhow!(
            "the cached wallpaper is missing; download it again"
        ));
    }
    let path = path.to_str().context("wallpaper path is not valid UTF-8")?;
    settings.set_string("active-file", path)?;
    catalog.set_last_active(&item.id)?;
    catalog.mark_used(&item.id, used_at)?;
    Ok(())
}

pub fn enforce_cache_limit(catalog: &Catalog, limit_bytes: u64) -> Result<()> {
    let entries = catalog.cache_entries()?;
    for id in eviction_plan(&entries, limit_bytes) {
        let item = catalog
            .get(&id)?
            .ok_or_else(|| anyhow!("cache entry disappeared: {id}"))?;
        remove_if_present(item.local_path.as_deref())?;
        remove_if_present(item.thumbnail_path.as_deref())?;
        catalog.clear_download(&id)?;
    }
    Ok(())
}

pub fn reconcile_cache(catalog: &Catalog) -> Result<()> {
    for item in catalog.list("", false)? {
        let missing_video = item
            .local_path
            .as_deref()
            .is_some_and(|path| !path.is_file());
        let missing_thumbnail = item.local_path.is_some()
            && item
                .thumbnail_path
                .as_deref()
                .is_none_or(|path| !path.is_file());
        if missing_video || missing_thumbnail {
            remove_if_present(item.local_path.as_deref())?;
            remove_if_present(item.thumbnail_path.as_deref())?;
            catalog.clear_download(&item.id)?;
        }
    }
    Ok(())
}

pub fn run_maintenance(paths: &AppPaths) -> Result<()> {
    let catalog = Catalog::open(&paths.database)?;
    reconcile_cache(&catalog)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let now = now_epoch();
    let settings = settings();
    let refresh_hours = settings
        .as_ref()
        .map_or(24, |settings| settings.uint("refresh-hours"));
    for query in catalog.searches_due(now, u64::from(refresh_hours) * 60 * 60)? {
        let results = runtime.block_on(discover(query.clone(), paths.previews.clone()))?;
        persist_search(&catalog, &query, results, now)?;
    }

    let mut rotation = catalog.rotation()?;
    if rotation.is_due(now)
        && let Some(item) = catalog.next_available(rotation.playlist_id, rotation.random)?
    {
        let settings = settings
            .as_ref()
            .context("Motion GSettings schema is not installed")?;
        apply(&catalog, settings, &item, now)?;
        rotation.last_change_epoch = now;
        catalog.set_rotation(&rotation)?;
    }
    let cache_limit = u64::from(settings.map_or(10, |settings| settings.uint("cache-limit-gb")));
    enforce_cache_limit(&catalog, cache_limit * 1024_u64 * 1024 * 1024)
}

#[must_use]
pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn file_size(path: &Path) -> Result<u64> {
    Ok(std::fs::metadata(path)?.len())
}

fn remove_if_present(path: Option<&Path>) -> Result<()> {
    if let Some(path) = path
        && let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error.into());
    }
    Ok(())
}
