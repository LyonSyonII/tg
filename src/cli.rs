use std::path::PathBuf;

use bpaf::Bpaf;

#[derive(Bpaf, Debug, Clone)]
#[bpaf(options, generate(parse))]
pub enum Cli {
    /// Adds TAGS to FILE, will create the tags that don't exist.
    ///
    /// Example: 'tg add Cargo.toml toml rust dev config'
    #[bpaf(command)]
    Add {
        #[bpaf(positional("FILE"))]
        file: PathBuf,
        #[bpaf(positional("TAGS"))]
        tags: Vec<String>,
    },
    /// Mounts the filesystem to the specified MOUNTPOINT or the previous one if skipped
    ///
    /// Example: 'tg mount ~/Tags'
    #[bpaf(command)]
    Mount {
        #[bpaf(positional("MOUNTPOINT"), optional)]
        mountpoint: Option<PathBuf>,
    },
    #[bpaf(command)]
    Set {
        #[bpaf(external)]
        set: Set,
    },
}

#[derive(Bpaf, Debug, Clone)]
pub enum Set {
    #[bpaf(command)]
    TagPrefix {
        #[bpaf(positional("VALUE"))]
        value: String,
    },
    #[bpaf(command)]
    FilePrefix {
        #[bpaf(positional("VALUE"))]
        value: String,
    },
}
