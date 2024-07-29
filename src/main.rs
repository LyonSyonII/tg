use fuse::Fuse;
use itertools::Itertools;
use std::{os::unix::ffi::OsStringExt, sync::atomic::AtomicBool};
use utils::Lazy;
use yakv::storage::Select;

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

static MOUNT_K: Lazy<Vec<u8>> = Lazy::new(|| b"__MOUNTPOINT__".to_vec());

fn main() -> Result<()> {
    let cli = <Cli as clap::Parser>::parse();

    let dirs = directories::ProjectDirs::from("dev", "lyonsyonii", "fg")
        .ok_or(anyhow!("Unable to create the application's data directory"))?;
    let local = dirs.data_dir();
    std::fs::create_dir_all(local)?;
    
    let db = yakv::storage::Storage::open(
        &local.join("db.yakv"),
        yakv::storage::StorageConfig::default(),
    ).context("Database")?;

    let mountpoint = db.get(MOUNT_K.get())?;

    let Some(command) = cli.command else {
        // no subcommand specified, add entry
        return add(cli.add, &db, mountpoint);
    };

    match command {
        Commands::Mount { mountpoint } => {
            mount(&mountpoint, db.iter())?;
            db.put(
                MOUNT_K.get().clone(),
                mountpoint.into_os_string().into_encoded_bytes(),
            )?;
        }
    }

    Ok(())
}

fn add(
    Add { file, tags }: Add,
    db: &yakv::storage::Storage,
    mountpoint: Option<Vec<u8>>,
) -> Result<()> {
    let file = file
        .ok_or_else(|| anyhow!("Called \"add\" without \"file\" argument"))?
        .canonicalize()?;
    eprintln!("Adding {file:?} : {tags:?}");

    if !file.try_exists()? {
        return Err(anyhow!("The file {file:?} does not exist"));
    }

    db.put(
        file.as_os_str().as_encoded_bytes().to_vec(),
        bitcode::encode(&tags),
    )?;

    if let Some(mountpoint) = mountpoint {
        let path: std::path::PathBuf = std::ffi::OsString::from_vec(mountpoint).into();
        add_entry(&path, &file, tags)?;
    }

    Ok(())
}

fn mount(mountpoint: &std::path::Path, entries: yakv::storage::StorageIterator) -> Result<()> {
    if !mountpoint.try_exists()? {
        return Err(anyhow!("The directory {mountpoint:?} does not exist"));
    }

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
    ).unwrap();
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

fn add_entry(
    mountpoint: &std::path::Path,
    file: &std::path::Path,
    mut tags: Vec<String>,
) -> Result<()> {
    debug_assert!(mountpoint.try_exists()?);
    for t in &mut tags {
        t.insert(0, '#')
    }

    for perm in tags.iter().permutations(tags.len()) {
        println!("permutations: {perm:?}");
    }

    Ok(())
}
