use std::{ffi::OsStr, time::UNIX_EPOCH};

use anyhow::Result;
use fuse3::path::prelude as fuse3;
use futures_util::StreamExt;
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

const LINK_ATTR: fuse3::FileAttr = fuse3::FileAttr {
    size: 0,
    blocks: 0,
    atime: UNIX_EPOCH, // 1970-01-01 00:00:00
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    kind: fuse3::FileType::Symlink,
    perm: 0o755,
    nlink: 2,
    uid: 1000,
    gid: 100,
    rdev: 0,
    blksize: 512,
};

pub struct Fuse {
    db_path: std::path::PathBuf,
    request_cache: tokio::sync::RwLock<std::collections::HashMap<u32, Vec<std::ffi::OsString>>>
}

impl Fuse {
    pub fn new(db_path: impl Into<std::path::PathBuf>) -> Self {
        Fuse {
            db_path: db_path.into(),
            request_cache: tokio::sync::RwLock::new(std::collections::HashMap::new())
        }
    }

    pub fn connect_db(&self) -> Result<rusqlite::Connection> {
        Ok(rusqlite::Connection::open(&self.db_path)?)
    }
}

type DynIter<T> = Box<dyn Iterator<Item = T> + Send>;

impl fuse3::PathFilesystem for Fuse {
    type DirEntryPlusStream<'a> = futures_util::stream::Iter<DynIter<::fuse3::Result<fuse3::DirectoryEntryPlus>>> where Self: 'a;
    type DirEntryStream<'a> = futures_util::stream::Iter<DynIter<::fuse3::Result<fuse3::DirectoryEntry>>> where Self: 'a;

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
        // eprintln!("[lookup]  parent = {parent:?}, name = {name:?}");

        Ok(fuse3::ReplyEntry {
            ttl: TTL,
            attr: ROOT_DIR_ATTR,
        })
    }

    async fn getattr(
        &self,
        _req: fuse3::Request,
        path: Option<&OsStr>,
        _fh: Option<u64>,
        _flags: u32,
    ) -> ::fuse3::Result<fuse3::ReplyAttr> {
        let path = path.map(std::path::Path::new);
        eprintln!("[getattr] path  = {path:?}");

        Ok(fuse3::ReplyAttr {
            ttl: TTL,
            attr: ROOT_DIR_ATTR,
        })
    }

    async fn readdirplus<'a>(
        &'a self,
        req: fuse3::Request,
        parent: &'a OsStr,
        fh: u64,
        offset: u64,
        lock_owner: u64,
    ) -> ::fuse3::Result<fuse3::ReplyDirectoryPlus<Self::DirEntryPlusStream<'a>>> {
        eprintln!("[readdirplus] path = {parent:?}, offset = {offset}");
        // dbg!(&req);

        let path = std::path::Path::new(parent);
        let tags = path
            .components()
            .skip(1)
            .map(|c| c.as_os_str());
        
        // let instant = std::time::Instant::now();
        
        let files = {
            let cache = self.request_cache.read().await.get(&req.pid).cloned();
            if let Some(c) = cache {
                c.get(offset as usize..).unwrap_or_default().to_vec()
            } else {
                let files = {
                    let db = rusqlite::Connection::open(&self.db_path).unwrap();
                    let (len, list) = tg::list_to_sql(tags);
                    let stmt = format!(
                        r#"
                        select file from FileTags
                        where tag in {list}
                        group by file 
                        having count(*) = {}
                        -- limit 100 offset {}
                        "#,
                        len,
                        offset
                    );
                    let mut stmt = db.prepare_cached(&stmt).unwrap();
                    stmt.query_map([], |r| {
                        let path = std::path::PathBuf::from(std::ffi::OsString::from(r.get::<_, String>(0)?));
                        Ok(path.file_name().unwrap().to_owned())
                    })
                    .unwrap()
                    .flatten()
                    .collect::<Vec<_>>()
                };

                self.request_cache.write().await.insert(req.pid, files.clone());
                files
            }
        }
        .into_iter()
        .map(|n| (fuse3::FileType::Symlink, n, LINK_ATTR));
        // eprintln!("Finished query and collect in {:?}\n", instant.elapsed());
        
        let entries = [
            (fuse3::FileType::Directory, std::ffi::OsString::from("."), ROOT_DIR_ATTR),
            (fuse3::FileType::Directory, std::ffi::OsString::from(".."), ROOT_DIR_ATTR),
        ]
        .into_iter()
        .chain(files)
        .enumerate()
        .map(|(i, (kind, name, attr))| {
            fuse3::DirectoryEntryPlus {
                kind,
                name,
                offset: i as i64 + 1,
                attr,
                entry_ttl: TTL,
                attr_ttl: TTL
            }
        })
        .skip(offset as usize)
        .map(::fuse3::Result::Ok);
        
        let entries: Box<dyn Iterator<Item = std::result::Result<fuse3::DirectoryEntryPlus, ::fuse3::Errno>> + Send> = Box::new(entries);
        Ok(fuse3::ReplyDirectoryPlus {
            entries: futures_util::stream::iter(entries),
        })
    }
}
