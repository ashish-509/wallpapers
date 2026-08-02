use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use futures_util::{StreamExt, stream};
use gtk::{gio, prelude::*};
use motion_core::{RemoteWallpaper, eviction_plan};
use motion_service::{
    Catalog, CatalogItem, DownloadClient, MediaOptimizer, RotationSettings, SourceConnector,
    WikimediaCommons,
};

use crate::paths::AppPaths;

const SETTINGS_ID: &str = "io.github.ashish509.motion";
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

/// Headless toggle of the live wallpaper engine, for a global keyboard shortcut.
pub fn run_toggle(_paths: &AppPaths) -> Result<()> {
    let settings = settings().context("the Motion settings schema is not installed")?;
    let enabled = !settings.boolean("enabled");
    settings.set_boolean("enabled", enabled)?;
    gio::Settings::sync();
    Ok(())
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

/// Advances to the next wallpaper in the active playlist and applies it
/// immediately, resetting the rotation clock so the timer counts from now.
pub fn apply_next(
    catalog: &Catalog,
    settings: &gio::Settings,
    now: u64,
) -> Result<Option<CatalogItem>> {
    apply_step(catalog, settings, now, true)
}

/// Returns to the previous wallpaper in the active playlist and applies it
/// immediately, resetting the rotation clock so the timer counts from now.
pub fn apply_prev(
    catalog: &Catalog,
    settings: &gio::Settings,
    now: u64,
) -> Result<Option<CatalogItem>> {
    apply_step(catalog, settings, now, false)
}

fn apply_step(
    catalog: &Catalog,
    settings: &gio::Settings,
    now: u64,
    forward: bool,
) -> Result<Option<CatalogItem>> {
    let mut rotation = catalog.rotation()?;
    let Some(item) = neighbour(catalog, &rotation, forward)? else {
        return Ok(None);
    };
    apply(catalog, settings, &item, now)?;
    rotation.last_change_epoch = now;
    catalog.set_rotation(&rotation)?;
    // Flush the queued active-file write so the extension reacts immediately.
    gio::Settings::sync();
    Ok(Some(item))
}

/// Picks the wallpaper before or after the current one in playlist order,
/// wrapping around at the ends. Falls back to any available wallpaper when no
/// ordered playlist is configured.
fn neighbour(
    catalog: &Catalog,
    rotation: &RotationSettings,
    forward: bool,
) -> Result<Option<CatalogItem>> {
    let Some(playlist_id) = rotation.playlist_id else {
        return Ok(catalog.next_available(None, rotation.random)?);
    };
    let items: Vec<CatalogItem> = catalog
        .playlist_items(playlist_id)?
        .into_iter()
        .filter(|item| item.local_path.as_deref().is_some_and(Path::is_file))
        .collect();
    if items.is_empty() {
        return Ok(None);
    }
    let active = catalog.last_active()?.unwrap_or_default();
    let current = items.iter().position(|item| item.id == active);
    let length = items.len();
    let index = match current {
        Some(index) if forward => (index + 1) % length,
        Some(index) => (index + length - 1) % length,
        None => 0,
    };
    Ok(Some(items[index].clone()))
}

/// Headless entry point for the `--next` / `--prev` CLI flags: switches the
/// desktop wallpaper without opening the window (used by global shortcuts).
pub fn run_step(paths: &AppPaths, forward: bool) -> Result<()> {
    let catalog = Catalog::open(&paths.database)?;
    let settings = settings().context("Motion GSettings schema is not installed")?;
    let now = now_epoch();
    let result = if forward {
        apply_next(&catalog, &settings, now)?
    } else {
        apply_prev(&catalog, &settings, now)?
    };
    match result {
        Some(item) => println!("now playing: {}", item.wallpaper.title),
        None => eprintln!("no other wallpaper is available to switch to"),
    }
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
        // Without a main loop the process would exit before the settings
        // backend flushes the queued active-file write.
        gio::Settings::sync();
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
