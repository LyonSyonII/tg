use std::{ffi::OsStr, hash::{Hash, Hasher}, os::unix::ffi::OsStrExt, time::UNIX_EPOCH};

use anyhow::Result;
use libc::ENOENT;
use rusqlite::Connection;
use fuse_mt as fusemt;

const TTL: std::time::Duration = std::time::Duration::from_secs(1); // 1 second

const ROOT_DIR_ATTR: fusemt::FileAttr = fusemt::FileAttr {
    size: 0,
    blocks: 0,
    atime: UNIX_EPOCH, // 1970-01-01 00:00:00
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    kind: fusemt::FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 1000,
    gid: 100,
    rdev: 0,

    // macOS only
    crtime: UNIX_EPOCH,
    flags: 0,
};

const LINK_ATTR: fusemt::FileAttr = fusemt::FileAttr {
    size: 0,
    blocks: 0,
    atime: UNIX_EPOCH, // 1970-01-01 00:00:00
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    kind: fusemt::FileType::Symlink,
    perm: 0o755,
    nlink: 2,
    uid: 1000,
    gid: 100,
    rdev: 0,

    // macOS only
    crtime: UNIX_EPOCH,
    flags: 0,
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

    pub fn tag_exists(&self, path: &std::path::Path) -> bool {
        let check_db = || {
            let db = self.connect_db().ok()?;
            let tag = path.file_name()?;
            db.query_row(
                "select 1 from FileTags where tag = ? limit 1", 
                [tag.to_string_lossy()], 
                |r| r.get::<_, u8>(0)
            ).ok()
        };
        
        path == std::path::Path::new("/") || check_db().is_some()
    }
}

impl fusemt::FilesystemMT for Fuse {
    fn getattr(&self, _req: fusemt::RequestInfo, path: &std::path::Path, _fh: Option<u64>) -> fusemt::ResultEntry {
        eprintln!("[getattr] path = {path:?}");
        
        if !self.tag_exists(path) {
            eprintln!("[getattr] Path {path:?} does not exist");
            return Err(libc::ENOENT);
        }

        Ok((TTL, ROOT_DIR_ATTR))
    }
    
    fn opendir(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, flags: u32) -> fuse_mt::ResultOpen {
        eprintln!("[opendir] path = {path:?}, flags = {:?}", flags.to_le_bytes());
        let mut hasher = std::hash::DefaultHasher::new();
        path.hash(&mut hasher);
        Ok((hasher.finish(), 0))
    }

    fn readdir(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, fh: u64) -> fuse_mt::ResultReaddir {
        eprintln!("[readdir] path = {path:?}, fh = {fh}");
        
        if !self.tag_exists(path) {
            eprintln!("[readdir] Path {path:?} does not exist");
            return Err(libc::ENOENT);
        }

        let tags = path
            .components()
            .skip(1)
            .map(|c| c.as_os_str());
        
        let db = rusqlite::Connection::open(&self.db_path).unwrap();
        let (len, list) = tg::list_to_sql(tags);
        let stmt = format!(
            r#"
            select file from FileTags
            where tag in {list}
            group by file 
            having count(*) = {}
            "#,
            len,
        );
        let mut stmt = db.prepare_cached(&stmt).unwrap();
        let files = stmt.query_map([], |r| {
            let name = std::ffi::OsString::from(r.get::<_, String>(0)?);
            let kind = fusemt::FileType::Symlink;
            Ok((name, kind))
        })
        .unwrap()
        .flatten();

        let entries = [
            (std::ffi::OsString::from("."), fusemt::FileType::Directory, ),
            (std::ffi::OsString::from(".."), fusemt::FileType::Directory, ),
        ]
        .into_iter()
        .chain(files)
        .map(|(name, kind)| fusemt::DirectoryEntry {
            name,
            kind
        })
        .collect();
    
        Ok(entries)
    }
    
    fn init(&self, _req: fuse_mt::RequestInfo) -> fuse_mt::ResultEmpty {
        eprintln!("[init] initialized filesystem");
        Ok(())
    }
    
    fn destroy(&self) {
        eprintln!("[destroy] destroyed filesystem");
    }
    
    fn chmod(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: Option<u64>, mode: u32) -> fuse_mt::ResultEmpty {
        eprintln!("[chmod] path = {path:?}, mode = {mode:#o}");
        Err(libc::ENOSYS)
    }
    
    fn chown(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: Option<u64>, uid: Option<u32>, gid: Option<u32>) -> fuse_mt::ResultEmpty {
        eprintln!("[chown] path = {path:?}, uid = {uid:?}, gid = {gid:?}");
        Err(libc::ENOSYS)
    }
    
    fn truncate(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: Option<u64>, size: u64) -> fuse_mt::ResultEmpty {
        eprintln!("[truncate] path = {path:?}, size = {size}");
        Err(libc::ENOSYS)
    }
    
    fn utimens(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: Option<u64>, _atime: Option<std::time::SystemTime>, _mtime: Option<std::time::SystemTime>) -> fuse_mt::ResultEmpty {
        eprintln!("[utimens] path = {path:?}");
        Err(libc::ENOSYS)
    }
    
    fn utimens_macos(&self, _req: fuse_mt::RequestInfo, _path: &std::path::Path, _fh: Option<u64>, _crtime: Option<std::time::SystemTime>, _chgtime: Option<std::time::SystemTime>, _bkuptime: Option<std::time::SystemTime>, _flags: Option<u32>) -> fuse_mt::ResultEmpty {
        Err(libc::ENOSYS)
    }
    
    fn readlink(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path) -> fuse_mt::ResultData {
        eprintln!("[readlink] path = {path:?}");
        Err(libc::ENOSYS)
    }
    
    fn mknod(&self, _req: fuse_mt::RequestInfo, parent: &std::path::Path, name: &OsStr, mode: u32, _rdev: u32) -> fuse_mt::ResultEntry {
        eprintln!("[mknod] parent = {parent:?}, name = {name:?}, mode = {mode:#o}");
        Err(libc::ENOSYS)
    }
    
    fn mkdir(&self, _req: fuse_mt::RequestInfo, parent: &std::path::Path, name: &OsStr, mode: u32) -> fuse_mt::ResultEntry {
        eprintln!("[mkdir] parent = {parent:?}, name = {name:?}, mode = {mode:#o}");
        Err(libc::ENOSYS)
    }
    
    fn unlink(&self, _req: fuse_mt::RequestInfo, parent: &std::path::Path, name: &OsStr) -> fuse_mt::ResultEmpty {
        eprintln!("[unlink] parent = {parent:?}, name = {name:?}");
        Err(libc::ENOSYS)
    }
    
    fn rmdir(&self, _req: fuse_mt::RequestInfo, parent: &std::path::Path, name: &OsStr) -> fuse_mt::ResultEmpty {
        eprintln!("[rmdir] parent = {parent:?}, name = {name:?}");
        Err(libc::ENOSYS)
    }
    
    fn symlink(&self, _req: fuse_mt::RequestInfo, parent: &std::path::Path, name: &OsStr, target: &std::path::Path) -> fuse_mt::ResultEntry {
        eprintln!("[symlink] parent = {parent:?}, name = {name:?}, target = {target:?}");
        Err(libc::ENOSYS)
    }
    
    fn rename(&self, _req: fuse_mt::RequestInfo, parent: &std::path::Path, name: &OsStr, newparent: &std::path::Path, newname: &OsStr) -> fuse_mt::ResultEmpty {
        eprintln!("[rename] parent = {parent:?}, name = {name:?}, newparent = {newparent:?}, newname = {newname:?}");
        Err(libc::ENOSYS)
    }
    
    fn link(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, newparent: &std::path::Path, newname: &OsStr) -> fuse_mt::ResultEntry {
        eprintln!("[link] parent = {path:?}, newparent = {newparent:?}, newname = {newname:?}");
        Err(libc::ENOSYS)
    }
    
    fn open(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, flags: u32) -> fuse_mt::ResultOpen {
        eprintln!("[open] path = {path:?}, flags = {:?}", flags.to_le_bytes());
        Err(libc::ENOSYS)
    }
    
    fn read(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: u64, offset: u64, size: u32, callback: impl FnOnce(fuse_mt::ResultSlice<'_>) -> fuse_mt::CallbackResult) -> fuse_mt::CallbackResult {
        eprintln!("[read] path = {path:?}, offset = {offset}, size = {size}");
        callback(Err(libc::ENOSYS))
    }
    
    fn write(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: u64, offset: u64, data: Vec<u8>, flags: u32) -> fuse_mt::ResultWrite {
        eprintln!("[write] path = {path:?}, offset = {offset}, data_len = {}, flags = {:?}", data.len(), flags.to_le_bytes());
        Err(libc::ENOSYS)
    }
    
    fn flush(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: u64, _lock_owner: u64) -> fuse_mt::ResultEmpty {
        eprintln!("[flush] path = {path:?}");
        Err(libc::ENOSYS)
    }
    
    fn release(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: u64, flags: u32, _lock_owner: u64, flush: bool) -> fuse_mt::ResultEmpty {
        eprintln!("[release] path = {path:?}, flags = {:?}, flush = {flush}", flags.to_le_bytes());
        Err(libc::ENOSYS)
    }
    
    fn fsync(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: u64, _datasync: bool) -> fuse_mt::ResultEmpty {
        eprintln!("[fsync] path = {path:?}");
        Err(libc::ENOSYS)
    }
    
    fn releasedir(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: u64, flags: u32) -> fuse_mt::ResultEmpty {
        eprintln!("[releasedir] path = {path:?}, flags = {:?}", flags.to_le_bytes());
        Err(libc::ENOSYS)
    }
    
    fn fsyncdir(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _fh: u64, datasync: bool) -> fuse_mt::ResultEmpty {
        eprintln!("[fsyncdir] path = {path:?}, datasync = {datasync}");
        Err(libc::ENOSYS)
    }
    
    fn statfs(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path) -> fuse_mt::ResultStatfs {
        eprintln!("[statfs] path = {path:?}");
        Err(libc::ENOSYS)
    }
    
    fn setxattr(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _name: &OsStr, _value: &[u8], _flags: u32, _position: u32) -> fuse_mt::ResultEmpty {
        eprintln!("[setxattr] path = {path:?}");
        Err(libc::ENOSYS)
    }
    
    fn getxattr(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _name: &OsStr, _size: u32) -> fuse_mt::ResultXattr {
        eprintln!("[getxattr] path = {path:?}");
        Err(libc::ENOSYS)
    }
    
    fn listxattr(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _size: u32) -> fuse_mt::ResultXattr {
        eprintln!("[listxattr] path = {path:?}");
        Err(libc::ENOSYS)
    }
    
    fn removexattr(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _name: &OsStr) -> fuse_mt::ResultEmpty {
        eprintln!("[removexattr] path = {path:?}");
        Err(libc::ENOSYS)
    }
    
    fn access(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path, _mask: u32) -> fuse_mt::ResultEmpty {
        eprintln!("[access] path = {path:?}");
        Err(libc::ENOSYS)
    }
    
    fn create(&self, _req: fuse_mt::RequestInfo, parent: &std::path::Path, name: &OsStr, _mode: u32, _flags: u32) -> fuse_mt::ResultCreate {
        eprintln!("[create] parent = {parent:?}, name = {name:?}");
        Err(libc::ENOSYS)
    }

    
}
