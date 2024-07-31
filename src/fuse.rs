use std::{
    ffi::OsStr,
    hash::{Hash, Hasher},
    os::unix::ffi::OsStrExt,
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result};
use fuse_mt as fusemt;
use log::debug;

use crate::Config;
use tg::ok_or_panic;

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
    config: Config,
}

impl Fuse {
    pub fn new(db_path: impl Into<std::path::PathBuf>, config: Config) -> Self {
        Fuse {
            db_path: db_path.into(),
            config,
        }
    }

    pub fn connect_db(&self) -> Result<rusqlite::Connection> {
        use rusqlite::OpenFlags;
        Ok(rusqlite::Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?)
    }

    pub fn connect_db_mut(&self) -> Result<rusqlite::Connection> {
        use rusqlite::OpenFlags;
        Ok(rusqlite::Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?)
    }

    pub fn tag_exists(&self, path: &std::path::Path) -> bool {
        let is_valid = |name: &std::ffi::OsStr| {
            let bytes = name.as_bytes();
            bytes.starts_with(self.config.file_prefix().as_bytes())
                || bytes.starts_with(self.config.tag_prefix().as_bytes())
        };

        let check_db = |tag: &std::ffi::OsStr| {
            let db = self.connect_db().ok()?;
            debug!("SEARCHING DATABASE FOR TAG {tag:?}");
            db.query_row(
                "select 1 from FileTags where tag = ? limit 1",
                [&tag.to_string_lossy()[1..]],
                |r| r.get::<_, u8>(0),
            )
            .ok()
        };

        match path.file_name() {
            Some(name) => is_valid(name) && check_db(name).is_some(),
            None => true,
        }
    }
}

impl fusemt::FilesystemMT for Fuse {
    fn getattr(
        &self,
        _req: fusemt::RequestInfo,
        path: &std::path::Path,
        _fh: Option<u64>,
    ) -> fusemt::ResultEntry {
        debug!("[getattr] path = {path:?}");

        if !self.tag_exists(path) {
            debug!("[getattr] Path {path:?} does not exist");
            return Err(libc::ENOENT);
        }

        Ok((TTL, ROOT_DIR_ATTR))
    }

    fn opendir(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        flags: u32,
    ) -> fuse_mt::ResultOpen {
        debug!(
            "[opendir] path = {path:?}, flags = {:?}",
            flags.to_le_bytes()
        );
        let mut hasher = std::hash::DefaultHasher::new();
        path.hash(&mut hasher);
        Ok((hasher.finish(), 0))
    }

    fn readdir(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        fh: u64,
    ) -> fuse_mt::ResultReaddir {
        // TODO: Allow configuring file and tag prefix
        debug!("[readdir] path = {path:?}, fh = {fh}");

        if !self.tag_exists(path) {
            debug!("[readdir] Path {path:?} does not exist");
            return Err(libc::ENOENT);
        }

        let tags = path
            .components()
            .skip(1)
            .flat_map(|c| std::str::from_utf8(c.as_os_str().as_bytes()))
            .map(|s| &s[1..]);

        let db = ok_or_panic!(
            self.connect_db(),
            "failed to connect to sqlite database in {:?}",
            self.db_path
        );
        let (tags_len, tags_list) = tg::list_to_sql(tags);
        let stmt = format!(
            r#"
                select file from FileTags
                where tag in {tags_list}
                group by file 
                having count(*) = {tags_len}
            "#
        );
        let mut prep_stmt = db.prepare_cached(&stmt).unwrap();
        let files = prep_stmt
            .query_map([], |r| {
                let name = std::ffi::OsString::from(format!("_{}", r.get_ref(0)?.as_str()?.split('/').last().unwrap()));
                let kind = fusemt::FileType::Symlink;
                Ok((name, kind))
            })
            .unwrap()
            .flatten();

        let stmt = if path.file_name().is_none() {
            "select distinct tag from FileTags;"
        } else {
            &format!(
                r#"
                select distinct tag from FileTags where file in (
                    {stmt}
                ) and tag not in {tags_list};
            "#
            )
        };
        let mut prep_stmt = db.prepare_cached(stmt).unwrap();
        let tags = prep_stmt
            .query_map([], |r| {
                let name = std::ffi::OsString::from(format!(":{}", r.get_ref(0)?.as_str()?));
                let kind = fusemt::FileType::Directory;
                Ok((name, kind))
            })
            .unwrap()
            .flatten();

        let entries = [
            (std::ffi::OsString::from("."), fusemt::FileType::Directory),
            (std::ffi::OsString::from(".."), fusemt::FileType::Directory),
        ]
        .into_iter()
        .chain(tags)
        .chain(files)
        .map(|(name, kind)| fusemt::DirectoryEntry { name, kind })
        .collect();

        Ok(entries)
    }

    fn init(&self, _req: fuse_mt::RequestInfo) -> fuse_mt::ResultEmpty {
        debug!("[init] initialized filesystem");
        Ok(())
    }

    fn destroy(&self) {
        debug!("[destroy] destroyed filesystem");
    }

    fn chmod(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: Option<u64>,
        mode: u32,
    ) -> fuse_mt::ResultEmpty {
        debug!("[chmod] path = {path:?}, mode = {mode:#o}");
        Err(libc::ENOSYS)
    }

    fn chown(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: Option<u64>,
        uid: Option<u32>,
        gid: Option<u32>,
    ) -> fuse_mt::ResultEmpty {
        debug!("[chown] path = {path:?}, uid = {uid:?}, gid = {gid:?}");
        Err(libc::ENOSYS)
    }

    fn truncate(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: Option<u64>,
        size: u64,
    ) -> fuse_mt::ResultEmpty {
        debug!("[truncate] path = {path:?}, size = {size}");
        Err(libc::ENOSYS)
    }

    fn utimens(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: Option<u64>,
        _atime: Option<std::time::SystemTime>,
        _mtime: Option<std::time::SystemTime>,
    ) -> fuse_mt::ResultEmpty {
        debug!("[utimens] path = {path:?}");
        Err(libc::ENOSYS)
    }

    fn utimens_macos(
        &self,
        _req: fuse_mt::RequestInfo,
        _path: &std::path::Path,
        _fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<u32>,
    ) -> fuse_mt::ResultEmpty {
        Err(libc::ENOSYS)
    }

    fn readlink(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path) -> fuse_mt::ResultData {
        debug!("[readlink] path = {path:?}");
        Err(libc::ENOSYS)
    }

    fn mknod(
        &self,
        _req: fuse_mt::RequestInfo,
        parent: &std::path::Path,
        name: &OsStr,
        mode: u32,
        _rdev: u32,
    ) -> fuse_mt::ResultEntry {
        debug!("[mknod] parent = {parent:?}, name = {name:?}, mode = {mode:#o}");
        Err(libc::ENOSYS)
    }

    fn mkdir(
        &self,
        _req: fuse_mt::RequestInfo,
        parent: &std::path::Path,
        name: &OsStr,
        mode: u32,
    ) -> fuse_mt::ResultEntry {
        debug!("[mkdir] parent = {parent:?}, name = {name:?}, mode = {mode:#o}");
        Err(libc::ENOSYS)
    }

    fn unlink(
        &self,
        _req: fuse_mt::RequestInfo,
        parent: &std::path::Path,
        name: &OsStr,
    ) -> fuse_mt::ResultEmpty {
        debug!("[unlink] parent = {parent:?}, name = {name:?}");
        Err(libc::ENOSYS)
    }

    fn rmdir(
        &self,
        _req: fuse_mt::RequestInfo,
        parent: &std::path::Path,
        name: &OsStr,
    ) -> fuse_mt::ResultEmpty {
        debug!("[rmdir] parent = {parent:?}, name = {name:?}");
        Err(libc::ENOSYS)
    }

    fn symlink(
        &self,
        _req: fuse_mt::RequestInfo,
        parent: &std::path::Path,
        name: &OsStr,
        target: &std::path::Path,
    ) -> fuse_mt::ResultEntry {
        debug!("[symlink] parent = {parent:?}, name = {name:?}, target = {target:?}");
        Err(libc::ENOSYS)
    }

    fn rename(
        &self,
        _req: fuse_mt::RequestInfo,
        parent: &std::path::Path,
        name: &OsStr,
        newparent: &std::path::Path,
        newname: &OsStr,
    ) -> fuse_mt::ResultEmpty {
        debug!("[rename] parent = {parent:?}, name = {name:?}, newparent = {newparent:?}, newname = {newname:?}");
        Err(libc::ENOSYS)
    }

    fn link(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        newparent: &std::path::Path,
        newname: &OsStr,
    ) -> fuse_mt::ResultEntry {
        debug!("[link] parent = {path:?}, newparent = {newparent:?}, newname = {newname:?}");
        Err(libc::ENOSYS)
    }

    fn open(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        flags: u32,
    ) -> fuse_mt::ResultOpen {
        debug!("[open] path = {path:?}, flags = {:?}", flags.to_le_bytes());
        Err(libc::ENOSYS)
    }

    fn read(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        offset: u64,
        size: u32,
        callback: impl FnOnce(fuse_mt::ResultSlice<'_>) -> fuse_mt::CallbackResult,
    ) -> fuse_mt::CallbackResult {
        debug!("[read] path = {path:?}, offset = {offset}, size = {size}");
        callback(Err(libc::ENOSYS))
    }

    fn write(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        offset: u64,
        data: Vec<u8>,
        flags: u32,
    ) -> fuse_mt::ResultWrite {
        debug!(
            "[write] path = {path:?}, offset = {offset}, data_len = {}, flags = {:?}",
            data.len(),
            flags.to_le_bytes()
        );
        Err(libc::ENOSYS)
    }

    fn flush(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        _lock_owner: u64,
    ) -> fuse_mt::ResultEmpty {
        debug!("[flush] path = {path:?}");
        Err(libc::ENOSYS)
    }

    fn release(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        flags: u32,
        _lock_owner: u64,
        flush: bool,
    ) -> fuse_mt::ResultEmpty {
        debug!(
            "[release] path = {path:?}, flags = {:?}, flush = {flush}",
            flags.to_le_bytes()
        );
        Err(libc::ENOSYS)
    }

    fn fsync(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        _datasync: bool,
    ) -> fuse_mt::ResultEmpty {
        debug!("[fsync] path = {path:?}");
        Err(libc::ENOSYS)
    }

    fn releasedir(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        flags: u32,
    ) -> fuse_mt::ResultEmpty {
        debug!(
            "[releasedir] path = {path:?}, flags = {:?}",
            flags.to_le_bytes()
        );
        Err(libc::ENOSYS)
    }

    fn fsyncdir(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _fh: u64,
        datasync: bool,
    ) -> fuse_mt::ResultEmpty {
        debug!("[fsyncdir] path = {path:?}, datasync = {datasync}");
        Err(libc::ENOSYS)
    }

    fn statfs(&self, _req: fuse_mt::RequestInfo, path: &std::path::Path) -> fuse_mt::ResultStatfs {
        debug!("[statfs] path = {path:?}");
        Err(libc::ENOSYS)
    }

    fn setxattr(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _name: &OsStr,
        _value: &[u8],
        _flags: u32,
        _position: u32,
    ) -> fuse_mt::ResultEmpty {
        debug!("[setxattr] path = {path:?}");
        Err(libc::ENOSYS)
    }

    fn getxattr(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _name: &OsStr,
        _size: u32,
    ) -> fuse_mt::ResultXattr {
        debug!("[getxattr] path = {path:?}");
        Err(libc::ENOSYS)
    }

    fn listxattr(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _size: u32,
    ) -> fuse_mt::ResultXattr {
        debug!("[listxattr] path = {path:?}");
        Err(libc::ENOSYS)
    }

    fn removexattr(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _name: &OsStr,
    ) -> fuse_mt::ResultEmpty {
        debug!("[removexattr] path = {path:?}");
        Err(libc::ENOSYS)
    }

    fn access(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        _mask: u32,
    ) -> fuse_mt::ResultEmpty {
        debug!("[access] path = {path:?}");
        Err(libc::ENOSYS)
    }

    fn create(
        &self,
        _req: fuse_mt::RequestInfo,
        parent: &std::path::Path,
        name: &OsStr,
        _mode: u32,
        _flags: u32,
    ) -> fuse_mt::ResultCreate {
        debug!("[create] parent = {parent:?}, name = {name:?}");
        Err(libc::ENOSYS)
    }
}
