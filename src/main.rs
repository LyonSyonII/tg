use clap::Parser;
use utils::Exit as _;
use yakv::storage::Select;

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
    Mount {
        mountpoint: String,
    },
}

#[derive(clap::Args, Debug, Clone)]
struct Add {
    file: String,
    #[clap(required = true)]
    tags: Vec<String>,
}

fn main() {
    let dirs = directories::ProjectDirs::from("dev", "lyonsyonii", "fg")
        .exit("error: unable to create app directory");
    let local = dirs.data_dir();
    std::fs::create_dir_all(local).unwrap();

    let db = yakv::storage::Storage::open(
        &local.join("db.yakv"),
        yakv::storage::StorageConfig::default(),
    )
    .unwrap();

    let cli = <Cli as clap::Parser>::parse();
    dbg!(&cli);
    let mountpoint = db
        .get(&b"".to_vec())
        .unwrap()
        .map(|v| String::from_utf8(v).unwrap());
    dbg!(mountpoint);
    
    if let Some(command) = cli.command {
        match command {
            Commands::Mount { mountpoint } => mount(mountpoint),
        }
    } else {
        add(cli.add);
    }
}

fn add(Add { file, tags }: Add) {
    eprintln!("Adding {file:?} : {tags:?}");
}

fn mount(mountpoint: String) {

}