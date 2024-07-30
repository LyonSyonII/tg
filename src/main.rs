use fuse::Fuse;
use std::{
    os::unix::ffi::{OsStrExt, OsStringExt},
    sync::atomic::AtomicBool,
};

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
    if !mountpoint.try_exists()? {
        return Err(anyhow!("The directory {mountpoint:?} does not exist"));
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(mount_async(fuse::Fuse::new(db_path), mountpoint))
}

async fn mount_async(fs: fuse::Fuse, mountpoint: &std::path::Path) -> Result<()> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    let mut mount_options = fuse3::MountOptions::default();
    mount_options.uid(uid).gid(gid).force_readdir_plus(true);
    
    let mut mount_handle = fuse3::path::Session::new(mount_options)
        .mount_with_unprivileged(fs, mountpoint)
        .await
        .unwrap();
    eprintln!("Mounted successfully to {mountpoint:?}");

    let handle = &mut mount_handle;

    tokio::select! {
        res = handle => {
            println!("[tg::unmounted] Unmounted manually");
            res.unwrap()
        },
        _ = tokio::signal::ctrl_c() => {
            eprintln!("[tg::unmounted] Process killed via signal");
            loop {
                // need to resort to Command as MountHandle::unmount can't be retried if an error happens
                let cmd = || std::process::Command::new("umount").arg(mountpoint.display().to_string()).output();
                let out = cmd()?;
                if out.status.success() {
                    eprintln!("[tg::unmount] Filesystem unmounted");
                    break;
                }
                eprintln!("[tg::unmount] Error {:?}", String::from_utf8(out.stderr).unwrap());
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
        }
    };
    Ok(())
}
