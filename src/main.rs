use anyhow::{anyhow, bail, Context, Result};
use fuse_mt as fusemt;
use log::{debug, info};
use rusqlite::ParamsFromIter;
use std::path::PathBuf;
use tg::cli::Cli;
use tg::config::Config;
use tg::{or_panic, sql};

#[cfg(not(target_os = "linux"))]
const IS_LINUX: () = const { compile_error!("[tg] This crate only works on Linux due to FUSE.") };

fn main() -> Result<()> {
    let cli = tg::cli::parse().run();

    let config = tg::config::Config::load()?;
    let db_path = config.db_path().to_path_buf();
    let mut db = rusqlite::Connection::open(&db_path).context("database creation failed")?;
    db.execute_batch(include_str!("./sql/migrations.sql"))?;

    let minimum_level = if matches!(cli, Cli::Mount { .. }) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Off
    };
    simple_logger::SimpleLogger::new()
        .with_level(minimum_level)
        .env()
        .init()?;

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
    let path = file.as_ref().canonicalize()?;
    let tags = tags.as_ref();

    debug!("Adding {path:?} : {tags:?}");

    if !path.try_exists()? {
        bail!("The file {path:?} does not exist");
    }

    let tx = db.transaction()?;
    {
        let path = path.to_string_lossy();
        tx.execute(sql::INSERT_FILE, [path.as_ref()])
            .context("could not insert file")?;
        let mut insert_tag_stmt = tx.prepare_cached(sql::INSERT_TAG)?;
        let mut insert_filetag_stmt = tx.prepare_cached(sql::INSERT_FILETAG)?;
        for tag in tags {
            insert_tag_stmt
                .execute([tag])
                .context("could not insert tag")?;
            insert_filetag_stmt
                .execute([path.as_ref(), tag.as_str()])
                .context("could not insert filetag")?;
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
        debug!(target: "unmount", "Process killed via signal");
    })?;
    let unmounted_extern = Arc::clone(&unmounted);
    std::thread::spawn(move || {
        // Ignore result of join(), allowing for system to be unmounted
        let _ = handle.guard.join();

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
