use anyhow::{anyhow, bail, Context, Result};
use fuse_mt as fusemt;
use log::{debug, info};
use rusqlite::ParamsFromIter;
use tg::or_panic;
use std::path::PathBuf;
use tg::cli::Cli;
use tg::config::Config;

#[cfg(not(target_os = "linux"))]
const IS_LINUX: () = const { compile_error!("[tg] This crate only works on Linux due to FUSE.") };

fn main() -> Result<()> {
    log::set_logger(&tg::utils::LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Debug);

    let cli = tg::cli::parse().run();

    let config = tg::config::Config::load()?;
    let db_path = config.db_path().to_path_buf();
    let mut db = rusqlite::Connection::open(&db_path).context("database creation failed")?;
    db.execute_batch(include_str!("./sql/migrations.sql"))?;

    match cli {
        Cli::Add { file, tags } => add(file, tags, &mut db)?,
        Cli::Mount { mountpoint } => mount(mountpoint, db_path, config)?,
        Cli::Set { set: s } => set(s, config)?,
    }

    Ok(())
}

fn add(
    file: impl AsRef<std::path::Path>,
    tags: impl AsRef<[String]>,
    db: &mut rusqlite::Connection,
) -> Result<()> {
    let file = file.as_ref().canonicalize()?;
    let tags = tags.as_ref();
    
    debug!("Adding {file:?} : {tags:?}");

    if !file.try_exists()? {
        bail!("The file {file:?} does not exist");
    }
    
    let tx = db.transaction()?;
    {
        let file = { 
            let name = file.file_name().ok_or_else(|| anyhow!("File with no name"))?.to_string_lossy();
            let stmt = tx.prepare_cached("SELECT count(id) FROM Files WHERE name = ?1")?;
            if stmt.exists([&name]) {
                
            } else {
                name
            }
        };

        tx.execute("INSERT OR IGNORE INTO Files (file) VALUES (?1)", [&file])?;
        tx.last_insert_rowid();
        let mut insert_tag_stmt = tx.prepare_cached("INSERT OR IGNORE INTO Tags (tag) VALUES (?1)")?;
        let mut insert_filetag_stmt = tx.prepare_cached(
            r#"INSERT OR IGNORE INTO FileTags (fileId, tagId) VALUES (
                (SELECT id FROM Files WHERE file = ?1),
                (SELECT id FROM Tags WHERE tag = ?2)
            )"#,
        )?;
        for tag in tags {
            insert_tag_stmt.execute([tag])?;
            insert_filetag_stmt.execute([file.as_ref(), tag.as_str()])?;
        }
    }
    tx.commit()?;
    
    Ok(())
}

/// Mounts the virtual filesystem.
///
/// Blocks until unmounted or interrupted.
fn mount(
    mountpoint: Option<PathBuf>,
    db_path: impl Into<std::path::PathBuf>,
    mut config: Config,
) -> Result<()> {
    use std::sync::{Arc, Condvar, Mutex};

    let mountpoint = match (mountpoint, config.mountpoint()) {
        (Some(mountpoint), _) => {
            if !mountpoint.try_exists()? {
                return Err(anyhow!("the directory {mountpoint:?} does not exist"));
            }
            config.set_mountpoint(mountpoint.clone())?;
            mountpoint
        }
        (None, Some(mountpoint)) => mountpoint.to_path_buf(),
        (None, None) => {
            anyhow::bail!("no default mountpoint found, set it with 'tg mount MOUNTPOINT'")
        }
    };

    let handle = fusemt::spawn_mount(
        fusemt::FuseMT::new(
            tg::fuse::Fuse::new(db_path, config),
            std::thread::available_parallelism()?.get(),
        ),
        &mountpoint,
        &[],
    )?;
    info!("Mounted successfully to {mountpoint:?}");

    let unmounted = Arc::new((Mutex::new(false), Condvar::new()));
    let killed = Arc::clone(&unmounted);
    ctrlc::set_handler(move || {
        let (lock, cvar) = &*killed;
        let mut killed = lock.lock().unwrap();
        *killed = true;
        cvar.notify_one();
        debug!("[tg::unmounted] Process killed via signal");
    })?;
    let unmounted_extern = Arc::clone(&unmounted);
    std::thread::spawn(move || {
        handle.guard.join().unwrap().unwrap();
        let (lock, cvar) = &*unmounted_extern;
        let mut unmounted = lock.lock().unwrap();
        if !*unmounted {
            *unmounted = true;
            cvar.notify_one();
            debug!("[tg::unmounted] Unmounted manually");
        }
    });

    let (lock, cvar) = &*unmounted;
    let mut unmounted = lock.lock().unwrap();
    while !*unmounted {
        unmounted = cvar.wait(unmounted).unwrap();
    }
    info!("Filesystem unmounted");

    Ok(())
}

fn set(key: tg::cli::Set, mut config: Config) -> Result<()> {
    let msg = format!("{key:?} set successfully");
    match key {
        tg::cli::Set::TagPrefix { value } => config.set_tag_prefix(value)?,
        tg::cli::Set::FilePrefix { value } => config.set_file_prefix(value)?,
    }
    info!("{msg}");
    Ok(())
}
