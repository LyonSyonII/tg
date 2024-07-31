use anyhow::Result;
use std::path::{Path, PathBuf};
use tg::or_panic;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Config {
    mountpoint: Option<PathBuf>,
    tag_prefix: String,
    file_prefix: String,

    #[serde(skip)]
    config_path: PathBuf,
    #[serde(skip)]
    db_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mountpoint: Default::default(),
            tag_prefix: String::from(":"),
            file_prefix: String::from("_"),
            config_path: PathBuf::new(),
            db_path: PathBuf::new(),
        }
    }
}

type ConfyResult = std::result::Result<(), confy::ConfyError>;

impl Config {
    pub fn load() -> Result<Self> {
        let dirs = get_dirs();
        let config_path = dirs.config_dir().join("config.toml");
        let data_path = dirs.data_dir().join("db.sqlite");

        let config = confy::load_path(&config_path)?;
        Ok(Config {
            config_path,
            db_path: data_path,
            ..config
        })
    }

    pub fn store(&self) -> ConfyResult {
        confy::store_path(self.config_path(), self)
    }

    pub fn mountpoint(&self) -> Option<&Path> {
        self.mountpoint.as_deref()
    }

    pub fn set_mountpoint(&mut self, mountpoint: PathBuf) -> ConfyResult {
        self.mountpoint = Some(mountpoint);
        self.store()
    }

    pub fn tag_prefix(&self) -> &str {
        &self.tag_prefix
    }

    pub fn set_tag_prefix(&mut self, tag_prefix: String) -> ConfyResult {
        self.tag_prefix = tag_prefix;
        self.store()
    }

    pub fn file_prefix(&self) -> &str {
        &self.file_prefix
    }

    pub fn set_file_prefix(&mut self, file_prefix: String) -> ConfyResult {
        self.file_prefix = file_prefix;
        self.store()
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

fn get_dirs() -> directories::ProjectDirs {
    or_panic!(
        directories::ProjectDirs::from("dev", "lyonsyonii", "tg"),
        "could not get app directories"
    )
}
