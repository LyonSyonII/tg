use std::path::PathBuf;

#[derive(clap::Parser, Debug)]
#[command(version, about, args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[clap(flatten)]
    pub add: Add,
}

impl Cli {
    pub fn parse() -> Self {
        <Cli as clap::Parser>::parse()
    }
}

#[derive(clap::Subcommand, Debug)]
pub enum Commands {
    /// Sets the mountpoint for the tag filesystem.
    Mount {
        mountpoint: Option<std::path::PathBuf>,
    },
    Set(Set),
}

#[derive(clap::Args, Debug)]
pub struct Add {
    #[clap(required = true)]
    pub file: Option<std::path::PathBuf>,
    #[clap(required = true)]
    pub tags: Vec<String>,
}

#[derive(clap::Args, Debug)]
pub struct Set {
    #[command(subcommand)]
    pub key: ConfigValues,
}

#[derive(clap::Subcommand, Debug)]
pub enum ConfigValues {
    TagPrefix { value: String },
    FilePrefix { value: String },
}
