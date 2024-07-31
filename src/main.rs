mod cli;
mod config;
mod fuse;
mod utils;

use anyhow::{anyhow, Context, Result};
use cli::{Cli, Commands};
use config::Config;
use fuse_mt as fusemt;
use log::{debug, info};

#[cfg(not(target_os = "linux"))]
const IS_LINUX: () = const { compile_error!("[tg] This crate only works on Linux due to FUSE.") };

fn main() -> Result<()> {
    log::set_logger(&tg::utils::LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Debug);

    let cli = Cli::parse();

    let mut config = config::Config::load()?;

    let db_path = config.db_path().to_path_buf();
    let db = rusqlite::Connection::open(&db_path).context("Database Creation")?;
    db.execute_batch(include_str!("./sql/migrations.sql"))?;

    let Some(command) = cli.command else {
        // no subcommand specified, add entry
        return add(cli.add, &db);
    };

    match command {
        Commands::Mount { mountpoint } => {
            if let Some(mountpoint) = mountpoint {
                if !mountpoint.try_exists()? {
                    return Err(anyhow!("the directory {mountpoint:?} does not exist"));
                }
                config.set_mountpoint(mountpoint)?;
            } else if config.mountpoint().is_none() {
                return Err(anyhow!(
                    "no default mountpoint found, set it with 'tg mount MOUNTPOINT'"
                ));
            }
            mount(db_path, config)?;
        }
        Commands::Set(cli::Set { key }) => {
            let msg = format!("{key:?} set successfully");
            match key {
                cli::ConfigValues::TagPrefix { value } => config.set_tag_prefix(value)?,
                cli::ConfigValues::FilePrefix { value } => config.set_file_prefix(value)?,
            }
            info!("{msg}");
        }
    }

    Ok(())
}

fn add(cli::Add { file, tags }: cli::Add, db: &rusqlite::Connection) -> Result<()> {
    let file = file
        .ok_or_else(|| anyhow!("Called \"add\" without \"file\" argument"))?
        .canonicalize()?;

    debug!("Adding {file:?} : {tags:?}");

    if !file.try_exists()? {
        return Err(anyhow!("The file {file:?} does not exist"));
    }

    let values = tg::list_to_values(file, tags);
    db.execute(&format!("insert into FileTags values {values};"), [])?;

    Ok(())
}

/// Mounts the virtual filesystem.
///
/// Blocks until unmounted or interrupted.
fn mount(db_path: impl Into<std::path::PathBuf>, config: Config) -> Result<()> {
    use std::sync::{Arc, Condvar, Mutex};
    let mountpoint = or_panic!(
        config.mountpoint().map(ToOwned::to_owned),
        "Called mount without mountpoint set"
    );
    
    let handle = fusemt::spawn_mount(
        fusemt::FuseMT::new(
            fuse::Fuse::new(db_path, config),
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
