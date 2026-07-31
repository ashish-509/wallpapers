//! One-off curation utility: seeds the catalog with a hand-picked playlist of
//! HD Wikimedia Commons live wallpapers and enables scheduled rotation.
//!
//! Usage: cargo run --example curate -- <data-dir> <cache-dir>
//!
//! The picks below were verified against Wikimedia Commons metadata (license,
//! resolution, size) and may need updating if the files are renamed. Re-running
//! is safe: already-downloaded picks are skipped and only missing ones fetched.

use std::error::Error;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use motion_service::{
    Catalog, DownloadClient, MediaOptimizer, RotationSettings, SourceConnector, WikimediaCommons,
};

const PLAYLIST: &str = "Coder Ambience";
const INTERVAL_MINUTES: u32 = 5;
const MAX_MEDIA_BYTES: u64 = 1024 * 1024 * 1024;

/// Search query paired with a unique substring of the wanted download URL.
const PICKS: [(&str, &str); 10] = [
    (
        "Radevormwald Raindrops on a window",
        "Radevormwald_-_Raindrops_on_a_window_07",
    ),
    (
        "Hong Kong night skyline cityscape",
        "Hong_Kong_Island_Night_Cityscape",
    ),
    (
        "Raindrops against the window night city Las Palmas",
        "Raindrops_against_the_window_in_the_night_city",
    ),
    ("Cars driving at night road", "Cars_driving_at_night"),
    (
        "Fireplace background motion texture",
        "Fantastic-fireplace-fire-chimney-hearth",
    ),
    (
        "Wolken Zeitraffer clouds timelapse",
        "Wolken_Zeitraffer_-_Clouds_Timelapse",
    ),
    (
        "Simulation Reveals Spiraling Supermassive Black Holes",
        "Spiraling_Supermassive_Black_Holes",
    ),
    ("Simulation A Disk Galaxy Forms", "A_Disk_Galaxy_Forms"),
    ("NASA Perpetual Ocean", "NASA_-_Perpetual_Ocean"),
    (
        "Northern Lights in Motion Timelapse of a Severe G4 Storm",
        "Northern_Lights_in_Motion",
    ),
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let usage = "usage: curate <data-dir> <cache-dir>";
    let mut arguments = std::env::args().skip(1);
    let data = PathBuf::from(arguments.next().ok_or(usage)?);
    let cache = PathBuf::from(arguments.next().ok_or(usage)?);

    let catalog = Catalog::open(&data.join("library.sqlite3"))?;
    let source = WikimediaCommons::new()?;
    let downloads = DownloadClient::new(MAX_MEDIA_BYTES)?;
    let optimizer = MediaOptimizer::default();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let playlist = match catalog
        .playlists()?
        .into_iter()
        .find(|playlist| playlist.name == PLAYLIST)
    {
        Some(playlist) => playlist.id,
        None => catalog.create_playlist(PLAYLIST)?,
    };

    let mut first = None;
    for (query, marker) in PICKS {
        let wallpaper = source
            .search(query, 16)
            .await?
            .into_iter()
            .find(|wallpaper| wallpaper.download_url.contains(marker))
            .ok_or_else(|| format!("no search result matches {marker}"))?;
        let id = catalog.upsert(&wallpaper)?;

        let cached = catalog
            .get(&id)?
            .and_then(|item| item.local_path)
            .is_some_and(|path| path.is_file());
        if cached {
            println!("cached:   {}", wallpaper.attribution());
        } else {
            println!("fetching: {}", wallpaper.attribution());
            let fetched = downloads
                .download(&wallpaper.download_url, &cache.join("downloads"))
                .await?;
            let result = optimizer.optimize(&fetched.path, &cache.join("media")).await;
            tokio::fs::remove_file(&fetched.path).await.ok();
            let media = result?;
            let bytes = std::fs::metadata(&media.video_path)?.len()
                + std::fs::metadata(&media.thumbnail_path)?.len();
            catalog.record_download(&id, &media.video_path, &media.thumbnail_path, bytes, now)?;
        }
        catalog.add_to_playlist(playlist, &id)?;
        if first.is_none() {
            first = Some(id);
        }
    }

    catalog.set_rotation(&RotationSettings {
        enabled: true,
        playlist_id: Some(playlist),
        random: true,
        interval_minutes: INTERVAL_MINUTES,
        last_change_epoch: 0,
    })?;

    let id = first.ok_or("nothing was curated")?;
    catalog.set_last_active(&id)?;
    catalog.mark_used(&id, now)?;
    if let Some(video) = catalog.get(&id)?.and_then(|item| item.local_path) {
        println!("ACTIVE={}", video.display());
    }
    Ok(())
}
