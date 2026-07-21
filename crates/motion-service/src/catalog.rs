use std::path::{Path, PathBuf};

use motion_core::{CacheEntry, RemoteWallpaper, Source};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogItem {
    pub id: String,
    pub wallpaper: RemoteWallpaper,
    pub favorite: bool,
    pub local_path: Option<PathBuf>,
    pub thumbnail_path: Option<PathBuf>,
    pub bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RotationSettings {
    pub enabled: bool,
    pub playlist_id: Option<i64>,
    pub random: bool,
    pub interval_minutes: u32,
    pub last_change_epoch: u64,
}

impl Default for RotationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            playlist_id: None,
            random: false,
            interval_minutes: 30,
            last_change_epoch: 0,
        }
    }
}

impl RotationSettings {
    #[must_use]
    pub fn is_due(&self, now_epoch: u64) -> bool {
        self.enabled
            && now_epoch.saturating_sub(self.last_change_epoch)
                >= u64::from(self.interval_minutes) * 60
    }
}

pub struct Catalog {
    connection: Connection,
}

impl Catalog {
    pub fn open(path: &Path) -> Result<Self, CatalogError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Self::from_connection(connection)
    }

    pub fn in_memory() -> Result<Self, CatalogError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, CatalogError> {
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS wallpapers (
                 id TEXT PRIMARY KEY,
                 metadata TEXT NOT NULL,
                 search_text TEXT NOT NULL,
                 favorite INTEGER NOT NULL DEFAULT 0,
                 local_path TEXT,
                 thumbnail_path TEXT,
                 bytes INTEGER NOT NULL DEFAULT 0,
                 last_used INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS playlists (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL UNIQUE
             );
             CREATE TABLE IF NOT EXISTS playlist_items (
                 playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                 wallpaper_id TEXT NOT NULL REFERENCES wallpapers(id) ON DELETE CASCADE,
                 position INTEGER NOT NULL,
                 PRIMARY KEY (playlist_id, wallpaper_id)
             );
             CREATE TABLE IF NOT EXISTS settings (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS saved_searches (
                 query TEXT PRIMARY KEY COLLATE NOCASE,
                 last_fetched INTEGER NOT NULL DEFAULT 0
             );",
        )?;
        Ok(Self { connection })
    }

    pub fn upsert(&self, wallpaper: &RemoteWallpaper) -> Result<String, CatalogError> {
        let id = wallpaper_id(wallpaper);
        let metadata = serde_json::to_string(wallpaper)?;
        let search_text = format!(
            "{} {} {}",
            wallpaper.title,
            wallpaper.author,
            wallpaper.categories.join(" ")
        );
        self.connection.execute(
            "INSERT INTO wallpapers (id, metadata, search_text)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET metadata = excluded.metadata, search_text = excluded.search_text",
            params![id, metadata, search_text],
        )?;
        Ok(id)
    }

    pub fn list(
        &self,
        query: &str,
        favorites_only: bool,
    ) -> Result<Vec<CatalogItem>, CatalogError> {
        let mut statement = self.connection.prepare(
            "SELECT id, metadata, favorite, local_path, thumbnail_path, bytes
             FROM wallpapers
             WHERE (?1 = '' OR instr(lower(search_text), lower(?1)) > 0)
               AND (?2 = 0 OR favorite = 1)
             ORDER BY favorite DESC, search_text COLLATE NOCASE
             LIMIT 200",
        )?;
        let rows = statement.query_map(params![query.trim(), favorites_only], row_to_item)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get(&self, id: &str) -> Result<Option<CatalogItem>, CatalogError> {
        self.connection
            .query_row(
                "SELECT id, metadata, favorite, local_path, thumbnail_path, bytes
                 FROM wallpapers WHERE id = ?1",
                [id],
                row_to_item,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn set_favorite(&self, id: &str, favorite: bool) -> Result<(), CatalogError> {
        ensure_changed(
            self.connection.execute(
                "UPDATE wallpapers SET favorite = ?2 WHERE id = ?1",
                params![id, favorite],
            )?,
            id,
        )
    }

    pub fn record_download(
        &self,
        id: &str,
        local_path: &Path,
        thumbnail_path: &Path,
        bytes: u64,
        used_at: u64,
    ) -> Result<(), CatalogError> {
        ensure_changed(
            self.connection.execute(
                "UPDATE wallpapers
                 SET local_path = ?2, thumbnail_path = ?3, bytes = ?4, last_used = ?5
                 WHERE id = ?1",
                params![
                    id,
                    local_path.to_string_lossy(),
                    thumbnail_path.to_string_lossy(),
                    bytes,
                    used_at
                ],
            )?,
            id,
        )
    }

    pub fn record_thumbnail(&self, id: &str, thumbnail_path: &Path) -> Result<(), CatalogError> {
        ensure_changed(
            self.connection.execute(
                "UPDATE wallpapers
                 SET thumbnail_path = CASE
                     WHEN local_path IS NULL THEN ?2
                     ELSE thumbnail_path
                 END
                 WHERE id = ?1",
                params![id, thumbnail_path.to_string_lossy()],
            )?,
            id,
        )
    }

    pub fn mark_used(&self, id: &str, used_at: u64) -> Result<(), CatalogError> {
        ensure_changed(
            self.connection.execute(
                "UPDATE wallpapers SET last_used = ?2 WHERE id = ?1",
                params![id, used_at],
            )?,
            id,
        )
    }

    pub fn clear_download(&self, id: &str) -> Result<(), CatalogError> {
        ensure_changed(
            self.connection.execute(
                "UPDATE wallpapers
                 SET local_path = NULL, thumbnail_path = NULL, bytes = 0
                 WHERE id = ?1",
                [id],
            )?,
            id,
        )
    }

    pub fn create_playlist(&self, name: &str) -> Result<i64, CatalogError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CatalogError::InvalidPlaylistName);
        }
        self.connection
            .execute("INSERT INTO playlists (name) VALUES (?1)", [name])?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>, CatalogError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name FROM playlists ORDER BY name COLLATE NOCASE")?;
        let rows = statement.query_map([], |row| {
            Ok(Playlist {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn add_to_playlist(
        &self,
        playlist_id: i64,
        wallpaper_id: &str,
    ) -> Result<(), CatalogError> {
        self.connection.execute(
            "INSERT INTO playlist_items (playlist_id, wallpaper_id, position)
             VALUES (?1, ?2, COALESCE((SELECT MAX(position) + 1 FROM playlist_items WHERE playlist_id = ?1), 0))
             ON CONFLICT(playlist_id, wallpaper_id) DO NOTHING",
            params![playlist_id, wallpaper_id],
        )?;
        Ok(())
    }

    pub fn playlist_items(&self, playlist_id: i64) -> Result<Vec<CatalogItem>, CatalogError> {
        let mut statement = self.connection.prepare(
            "SELECT w.id, w.metadata, w.favorite, w.local_path, w.thumbnail_path, w.bytes
             FROM playlist_items p
             JOIN wallpapers w ON w.id = p.wallpaper_id
             WHERE p.playlist_id = ?1
             ORDER BY p.position",
        )?;
        let rows = statement.query_map([playlist_id], row_to_item)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn set_last_active(&self, id: &str) -> Result<(), CatalogError> {
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES ('last_active', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [id],
        )?;
        Ok(())
    }

    pub fn last_active(&self) -> Result<Option<String>, CatalogError> {
        self.connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'last_active'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn rotation(&self) -> Result<RotationSettings, CatalogError> {
        let value = self
            .connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'rotation'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        value.map_or_else(
            || Ok(RotationSettings::default()),
            |value| serde_json::from_str(&value).map_err(Into::into),
        )
    }

    pub fn set_rotation(&self, rotation: &RotationSettings) -> Result<(), CatalogError> {
        let value = serde_json::to_string(rotation)?;
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES ('rotation', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [value],
        )?;
        Ok(())
    }

    pub fn remember_search(&self, query: &str, fetched_at: u64) -> Result<(), CatalogError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(());
        }
        self.connection.execute(
            "INSERT INTO saved_searches (query, last_fetched) VALUES (?1, ?2)
             ON CONFLICT(query) DO UPDATE SET last_fetched = excluded.last_fetched",
            params![query, fetched_at],
        )?;
        Ok(())
    }

    pub fn saved_searches(&self) -> Result<Vec<String>, CatalogError> {
        let mut statement = self.connection.prepare(
            "SELECT query FROM saved_searches ORDER BY last_fetched, query COLLATE NOCASE LIMIT 20",
        )?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn searches_due(
        &self,
        now_epoch: u64,
        interval_seconds: u64,
    ) -> Result<Vec<String>, CatalogError> {
        let mut statement = self.connection.prepare(
            "SELECT query FROM saved_searches
             WHERE ?1 >= last_fetched AND ?1 - last_fetched >= ?2
             ORDER BY last_fetched, query COLLATE NOCASE
             LIMIT 20",
        )?;
        let rows = statement.query_map(params![now_epoch, interval_seconds], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn next_available(
        &self,
        playlist_id: Option<i64>,
        random: bool,
    ) -> Result<Option<CatalogItem>, CatalogError> {
        let active = self.last_active()?.unwrap_or_default();
        let order = if random {
            "RANDOM()"
        } else {
            "(w.id <= ?2), w.id"
        };
        let sql = if playlist_id.is_some() {
            format!(
                "SELECT w.id, w.metadata, w.favorite, w.local_path, w.thumbnail_path, w.bytes
                 FROM wallpapers w
                 JOIN playlist_items p ON p.wallpaper_id = w.id
                 WHERE p.playlist_id = ?1 AND w.local_path IS NOT NULL AND w.id != ?2
                 ORDER BY {order} LIMIT 1"
            )
        } else {
            format!(
                "SELECT w.id, w.metadata, w.favorite, w.local_path, w.thumbnail_path, w.bytes
                 FROM wallpapers w
                 WHERE (?1 IS NULL) AND w.local_path IS NOT NULL AND w.id != ?2
                 ORDER BY {order} LIMIT 1"
            )
        };
        self.connection
            .query_row(&sql, params![playlist_id, active], row_to_item)
            .optional()
            .map_err(Into::into)
    }

    pub fn cache_entries(&self) -> Result<Vec<CacheEntry>, CatalogError> {
        let active = self.last_active()?;
        let mut statement = self.connection.prepare(
            "SELECT w.id, w.bytes, w.last_used, w.favorite,
                    EXISTS(SELECT 1 FROM playlist_items p WHERE p.wallpaper_id = w.id)
             FROM wallpapers w WHERE w.local_path IS NOT NULL",
        )?;
        let rows = statement.query_map([], |row| {
            let id = row.get::<_, String>(0)?;
            Ok(CacheEntry {
                pinned: row.get::<_, bool>(3)?
                    || row.get::<_, bool>(4)?
                    || active.as_ref() == Some(&id),
                id,
                bytes: row.get(1)?,
                last_used_epoch: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogItem> {
    let metadata = row.get::<_, String>(1)?;
    let wallpaper = serde_json::from_str(&metadata).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            metadata.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    Ok(CatalogItem {
        id: row.get(0)?,
        wallpaper,
        favorite: row.get(2)?,
        local_path: row.get::<_, Option<String>>(3)?.map(PathBuf::from),
        thumbnail_path: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
        bytes: row.get(5)?,
    })
}

fn wallpaper_id(wallpaper: &RemoteWallpaper) -> String {
    let source = match wallpaper.source {
        Source::WikimediaCommons => "commons",
        Source::LocalImport => "local",
    };
    format!("{source}:{}", wallpaper.source_id)
}

fn ensure_changed(changed: usize, id: &str) -> Result<(), CatalogError> {
    if changed == 0 {
        Err(CatalogError::NotFound(id.to_owned()))
    } else {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("catalog file failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("catalog metadata failed: {0}")]
    Metadata(#[from] serde_json::Error),
    #[error("wallpaper not found: {0}")]
    NotFound(String),
    #[error("playlist name cannot be empty")]
    InvalidPlaylistName,
}

#[cfg(test)]
mod tests {
    use motion_core::License;

    use super::*;

    fn wallpaper(id: &str, title: &str) -> RemoteWallpaper {
        RemoteWallpaper {
            source: Source::WikimediaCommons,
            source_id: id.into(),
            title: title.into(),
            author: "Example Author".into(),
            description_url: format!("https://commons.wikimedia.org/wiki/File:{id}"),
            download_url: format!("https://upload.wikimedia.org/{id}.webm"),
            thumbnail_url: format!("https://upload.wikimedia.org/{id}.jpg"),
            mime_type: "video/webm".into(),
            license: License::from_commons(
                "CC BY 4.0",
                "https://creativecommons.org/licenses/by/4.0",
            )
            .unwrap(),
            categories: vec!["Nature".into(), "Rain".into()],
        }
    }

    #[test]
    fn stores_searches_favorites_playlists_and_restore_state() {
        let catalog = Catalog::in_memory().unwrap();
        let rain_id = catalog.upsert(&wallpaper("rain", "Summer rain")).unwrap();
        catalog.upsert(&wallpaper("space", "Deep space")).unwrap();

        assert_eq!(catalog.list("Summer", false).unwrap().len(), 1);
        assert_eq!(catalog.list("rain", false).unwrap().len(), 2);
        catalog.set_favorite(&rain_id, true).unwrap();
        assert_eq!(catalog.list("", true).unwrap()[0].id, rain_id);

        let playlist = catalog.create_playlist("Quiet").unwrap();
        catalog.add_to_playlist(playlist, &rain_id).unwrap();
        assert_eq!(catalog.playlist_items(playlist).unwrap()[0].id, rain_id);
        assert_eq!(
            catalog.playlists().unwrap(),
            vec![Playlist {
                id: playlist,
                name: "Quiet".into()
            }]
        );

        catalog.set_last_active(&rain_id).unwrap();
        assert_eq!(
            catalog.last_active().unwrap().as_deref(),
            Some(rain_id.as_str())
        );
    }

    #[test]
    fn persists_rotation_saved_searches_and_next_selection() {
        let catalog = Catalog::in_memory().unwrap();
        let first = catalog.upsert(&wallpaper("first", "First")).unwrap();
        let second = catalog.upsert(&wallpaper("second", "Second")).unwrap();
        for id in [&first, &second] {
            catalog
                .record_download(id, Path::new("video.webm"), Path::new("thumb.jpg"), 50, 10)
                .unwrap();
        }
        catalog.set_last_active(&first).unwrap();

        let rotation = RotationSettings {
            enabled: true,
            playlist_id: None,
            random: false,
            interval_minutes: 15,
            last_change_epoch: 100,
        };
        catalog.set_rotation(&rotation).unwrap();
        catalog.remember_search("nature", 20).unwrap();
        catalog.remember_search("rain", 10).unwrap();

        assert_eq!(catalog.get(&first).unwrap().unwrap().id, first);
        assert_eq!(catalog.rotation().unwrap(), rotation);
        assert_eq!(catalog.saved_searches().unwrap(), vec!["rain", "nature"]);
        assert_eq!(catalog.searches_due(100, 85).unwrap(), vec!["rain"]);
        assert_eq!(
            catalog.next_available(None, false).unwrap().unwrap().id,
            second
        );
        assert!(catalog.rotation().unwrap().is_due(1000));
    }

    #[test]
    fn remote_refresh_preserves_an_optimized_thumbnail() {
        let catalog = Catalog::in_memory().unwrap();
        let id = catalog.upsert(&wallpaper("rain", "Summer rain")).unwrap();
        catalog
            .record_download(
                &id,
                Path::new("video.webm"),
                Path::new("optimized.jpg"),
                50,
                10,
            )
            .unwrap();
        catalog
            .record_thumbnail(&id, Path::new("remote.jpg"))
            .unwrap();
        assert_eq!(
            catalog.get(&id).unwrap().unwrap().thumbnail_path,
            Some(PathBuf::from("optimized.jpg"))
        );
    }

    #[test]
    fn pins_favorites_playlist_items_and_active_wallpaper_in_cache() {
        let catalog = Catalog::in_memory().unwrap();
        let id = catalog.upsert(&wallpaper("rain", "Summer rain")).unwrap();
        catalog
            .record_download(&id, Path::new("video.webm"), Path::new("thumb.jpg"), 50, 10)
            .unwrap();
        catalog.set_last_active(&id).unwrap();
        assert!(catalog.cache_entries().unwrap()[0].pinned);
    }
}
