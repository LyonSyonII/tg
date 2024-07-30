use fuse::Fuse;
use std::{os::unix::ffi::OsStringExt, sync::atomic::AtomicBool};

use anyhow::{anyhow, Context, Result};

mod fuse;
mod utils;

#[derive(clap::Parser, Debug, Clone)]
#[command(version, about, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[clap(flatten)]
    add: Add,
}

#[derive(clap::Subcommand, Debug, Clone)]
enum Commands {
    /// Sets the mountpoint for the tag filesystem.
    Mount { mountpoint: std::path::PathBuf },
}

#[derive(clap::Args, Debug, Clone)]
struct Add {
    #[clap(required = true)]
    file: Option<std::path::PathBuf>,
    #[clap(required = true)]
    tags: Vec<String>,
}

struct Config {
    mountpoint: Option<std::path::PathBuf>,
}

fn main() -> Result<()> {
    let cli = <Cli as clap::Parser>::parse();

    let dirs = directories::ProjectDirs::from("dev", "lyonsyonii", "fg")
        .ok_or(anyhow!("Unable to create the application's data directory"))?;
    let local = dirs.data_dir();
    std::fs::create_dir_all(local)?;
    let db_path = local.join("db.sqlite");

    let db = rusqlite::Connection::open(&db_path).context("Database Creation")?;

    db.execute_batch(include_str!("./sql/migrations.sql"))?;

    let config = db
        .query_row("select * from Config", [], |r| {
            let mountpoint = r
                .get(1)
                .ok()
                .map(|m: String| std::ffi::OsString::from_vec(m.into_bytes()).into());
            Ok(Config { mountpoint })
        })
        .context("Get Config")?;

    let Some(command) = cli.command else {
        // no subcommand specified, add entry
        return add(cli.add, &db);
    };

    match command {
        Commands::Mount { mountpoint } => {
            mount(&mountpoint, db_path)?;
            db.execute(
                "update Config set mountpoint = ?",
                [mountpoint.display().to_string()],
            )?;
        }
    }

    Ok(())
}

fn add(Add { file, tags }: Add, db: &rusqlite::Connection) -> Result<()> {
    let file = file
        .ok_or_else(|| anyhow!("Called \"add\" without \"file\" argument"))?
        .canonicalize()?;
    eprintln!("Adding {file:?} : {tags:?}");

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
fn mount(mountpoint: &std::path::Path, db_path: impl AsRef<std::path::Path>) -> Result<()> {
    use std::sync::*;

    if !mountpoint.try_exists()? {
        return Err(anyhow!("The directory {mountpoint:?} does not exist"));
    }

    let pair = Arc::new((Mutex::new(false), Condvar::new()));
    let pair2 = Arc::clone(&pair);
    let pair3 = Arc::clone(&pair);
    ctrlc::set_handler(move || {
        let (lock, cvar) = &*pair2;
        let mut started = lock.lock().unwrap();
        *started = true;
        // We notify the condvar that the value has changed.
        cvar.notify_one();
    })
    .unwrap();

    let handle = fuser::spawn_mount2(
        Fuse::new(db_path)?,
        mountpoint,
        &[fuser::MountOption::FSName("tg".to_owned())],
    )
    .unwrap();
    eprintln!("Mounted successfully to {mountpoint:?}");

    let handle = std::sync::Arc::new(std::sync::Mutex::new(Some(handle)));
    let handle2 = handle.clone();
    std::thread::spawn(move || {
        loop {
            let finished = handle2
                .lock()
                .unwrap()
                .as_ref()
                .map(|h| h.guard.is_finished());
            match finished {
                Some(true) => break,
                Some(false) => {
                    std::thread::yield_now();
                    continue;
                }
                None => {
                    eprintln!("[Unmounted] Process killed via signal");
                    return
                },
            }
        }
        
        eprintln!("[Unmounted] Unmounted manually");
        let (lock, cvar) = &*pair3;
        let mut started = lock.lock().unwrap();
        *started = true;
        // We notify the condvar that the value has changed.
        cvar.notify_one();
    });

    // Wait for the threads to end.
    let (lock, cvar) = &*pair;
    let mut started = lock.lock().unwrap();
    while !*started {
        started = cvar.wait(started).unwrap();
    }

    if let Some(handle) = handle.lock().unwrap().take() {
        handle.join();
    }

    Ok(())
}
