use std::{ffi::OsStr, time::UNIX_EPOCH};

use anyhow::Result;
use fuse3::path::prelude as fuse3;
use libc::ENOENT;
use rusqlite::Connection;

const TTL: std::time::Duration = std::time::Duration::from_secs(1); // 1 second

const ROOT_DIR_ATTR: fuse3::FileAttr = fuse3::FileAttr {
    size: 0,
    blocks: 0,
    atime: UNIX_EPOCH, // 1970-01-01 00:00:00
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    kind: fuse3::FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 1000,
    gid: 100,
    rdev: 0,
    blksize: 512,
};

pub struct Fuse {
    db_path: std::path::PathBuf,
}

impl Fuse {
    pub fn new(db_path: impl Into<std::path::PathBuf>) -> Self {
        Fuse {
            db_path: db_path.into(),
        }
    }

    pub fn connect_db(&self) -> Result<rusqlite::Connection> {
        Ok(rusqlite::Connection::open(&self.db_path)?)
    }
}

impl fuse3::PathFilesystem for Fuse {
    type DirEntryStream<'a> = futures_util::stream::Empty<::fuse3::Result<fuse3::DirectoryEntry>> where Self:'a;
    type DirEntryPlusStream<'a> = futures_util::stream::Empty<::fuse3::Result<fuse3::DirectoryEntryPlus>> where Self: 'a;

    async fn init(&self, _req: fuse3::Request) -> ::fuse3::Result<fuse3::ReplyInit> {
        Ok(fuse3::ReplyInit {
            max_write: std::num::NonZeroU32::new(16 * 1024).unwrap(),
        })
    }

    async fn destroy(&self, _req: fuse3::Request) {
        eprintln!("Destroyed");
    }

    async fn lookup(
        &self,
        _req: fuse3::Request,
        parent: &OsStr,
        name: &OsStr,
    ) -> ::fuse3::Result<fuse3::ReplyEntry> {
        let parent = std::path::Path::new(parent);
        let name = std::path::Path::new(name);
        eprintln!("[lookup]  parent = {parent:?}, name = {name:?}");

        Ok(fuse3::ReplyEntry {
            ttl: TTL,
            attr: ROOT_DIR_ATTR,
        })
    }
    
    async fn getattr(&self, _req:fuse3::Request,path:Option<&OsStr>, _fh:Option<u64>, _flags:u32,) -> ::fuse3::Result<fuse3::ReplyAttr> {
        let path = path.map(std::path::Path::new);
        eprintln!("[getattr] path  = {path:?}");

        Ok(
            fuse3::ReplyAttr {
                ttl: TTL,
                attr: ROOT_DIR_ATTR,
            }
        )
    }
}
