use std::path::PathBuf;

use anyhow::Result;
use gtk::glib;

const DIRECTORY_NAME: &str = "io.github.ashish509.Motion";

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub database: PathBuf,
    pub downloads: PathBuf,
    pub media: PathBuf,
    pub previews: PathBuf,
}

impl AppPaths {
    pub fn create() -> Result<Self> {
        let data = glib::user_data_dir().join(DIRECTORY_NAME);
        let cache = glib::user_cache_dir().join(DIRECTORY_NAME);
        let paths = Self {
            database: data.join("library.sqlite3"),
            downloads: cache.join("downloads"),
            media: cache.join("media"),
            previews: cache.join("previews"),
        };
        std::fs::create_dir_all(&data)?;
        std::fs::create_dir_all(&paths.downloads)?;
        std::fs::create_dir_all(&paths.media)?;
        std::fs::create_dir_all(&paths.previews)?;
        Ok(paths)
    }
}
