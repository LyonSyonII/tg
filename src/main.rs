use fuse::Fuse;
use std::{os::unix::ffi::OsStringExt, sync::atomic::AtomicBool};
use utils::Lazy;

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

    let db = rusqlite::Connection::open(local.join("db.sqlite")).context("Database Creation")?;

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
        return add(cli.add, &db, config.mountpoint.as_deref());
    };

    match command {
        Commands::Mount { mountpoint } => {
            mount(&mountpoint, &db)?;
            db.execute(
                "update Config set mountpoint = ?",
                [mountpoint.display().to_string()],
            )?;
        }
    }

    Ok(())
}

fn add(
    Add { file, tags }: Add,
    db: &rusqlite::Connection,
    mountpoint: Option<&std::path::Path>,
) -> Result<()> {
    let file = file
        .ok_or_else(|| anyhow!("Called \"add\" without \"file\" argument"))?
        .canonicalize()?;
    eprintln!("Adding {file:?} : {tags:?}");

    if !file.try_exists()? {
        return Err(anyhow!("The file {file:?} does not exist"));
    }
    
    let separator = format!("\"),({file:?},\"");
    db.execute(
        &format!(
            "insert into FileTags values ({file:?},\"{}\");",
            tags.join(&separator)
        ),
        [],
    )?;
    // db.put(
    //     file.as_os_str().as_encoded_bytes().to_vec(),
    //     bitcode::encode(&tags),
    // )?;

    Ok(())
}

fn mount(mountpoint: &std::path::Path, db: &rusqlite::Connection) -> Result<()> {
    if !mountpoint.try_exists()? {
        return Err(anyhow!("The directory {mountpoint:?} does not exist"));
    }

    let mut files_stmt = db.prepare_cached("select distinct file from FileTags")?;
    let rows = files_stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut tags_stmt = db.prepare_cached("select tag from FileTags where file = ?")?;
    for file in rows.flatten() {
        let tags = tags_stmt
            .query_map([&file], |r| r.get::<_, String>(0))?
            .flatten()
            .collect::<Vec<_>>();
        println!("{file}: {tags:?}");
    }

    return Ok(());

    let running = std::sync::Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .unwrap();

    let handle = fuser::spawn_mount2(
        Fuse,
        mountpoint,
        &[fuser::MountOption::FSName("tg".to_owned())],
    )
    .unwrap();
    println!("Mounted successfully to {mountpoint:?}");

    while running.load(std::sync::atomic::Ordering::SeqCst) {}

    drop(handle);

    // for (file, tags) in entries.flatten() {
    //     let file: std::path::PathBuf = std::ffi::OsString::from_vec(file).into();
    //     if !file.has_root() {
    //         continue;
    //     }
    //     let tags: Vec<String> = bitcode::decode(&tags)?;
    //     add_entry(mountpoint, &file, tags)?;
    // }

    Ok(())
}
