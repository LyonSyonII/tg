use fuse::Fuse;
use std::os::unix::prelude::OsStringExt;
use fuse_mt as fusemt;
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

struct ConsoleLogger;

impl log::Log for ConsoleLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }
    
    fn log(&self, record: &log::Record<'_>) {
        println!("{}: {}: {}", record.target(), record.level(), record.args());
    }
    
    fn flush(&self) {}
}

static LOGGER: ConsoleLogger = ConsoleLogger;

fn main() -> Result<()> {

    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Debug);

    let cli = <Cli as clap::Parser>::parse();

    let dirs = directories::ProjectDirs::from("dev", "lyonsyonii", "tg")
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
fn mount(mountpoint: &std::path::Path, db_path: impl Into<std::path::PathBuf>) -> Result<()> {
    use std::sync::{ Arc, Mutex, Condvar };
    if !mountpoint.try_exists()? {
        return Err(anyhow!("The directory {mountpoint:?} does not exist"));
    }
    
    let handle = fusemt::spawn_mount(fusemt::FuseMT::new(fuse::Fuse::new(db_path), std::thread::available_parallelism()?.get()), mountpoint, &[])?;
    eprintln!("Mounted successfully to {mountpoint:?}");

    let unmounted = Arc::new((Mutex::new(false), Condvar::new()));
    let killed = Arc::clone(&unmounted);
    ctrlc::set_handler(move || {
        let (lock, cvar) = &*killed;
        let mut killed = lock.lock().unwrap();
        *killed = true;
        // We notify the condvar that the value has changed.
        cvar.notify_one();
        eprintln!("[tg::unmounted] Process killed via signal");
    })?;
    let unmounted_extern = Arc::clone(&unmounted);
    std::thread::spawn(move || {
        handle.guard.join().unwrap().unwrap();
        let (lock, cvar) = &*unmounted_extern;
        let mut unmounted = lock.lock().unwrap();
        *unmounted = true;
        // We notify the condvar that the value has changed.
        cvar.notify_one();
        eprintln!("[tg::unmounted] Unmounted manually");
    });
    
    let (lock, cvar) = &*unmounted;
    let mut unmounted = lock.lock().unwrap();
    while !*unmounted {
        unmounted = cvar.wait(unmounted).unwrap();
    }
    eprintln!("[tg::unmount] Filesystem unmounted");
    
    Ok(())
}
