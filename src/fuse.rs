use std::{
    ffi::{OsStr, OsString},
    hash::{Hash, Hasher},
    os::unix::ffi::OsStrExt,
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result};
use fuse_mt as fusemt;
use log::{debug, info};

use crate::{config::Config, ok_or_panic};

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

#[derive(Debug)]
enum Name<'a> {
    File(&'a OsStr),
    Tag(&'a OsStr),
    Root,
    None,
}

impl Fuse {
    pub fn new(db_path: impl Into<std::path::PathBuf>, config: Config) -> Self {
        Fuse {
            db_path: db_path.into(),
            config,
        }
    }

    fn connect_db(&self) -> rusqlite::Result<rusqlite::Connection> {
        use rusqlite::OpenFlags;
        rusqlite::Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
    }

    fn connect_db_mut(&self) -> rusqlite::Result<rusqlite::Connection> {
        use rusqlite::OpenFlags;
        rusqlite::Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
    }

    fn name_exists<'a>(&self, path: &'a std::path::Path) -> rusqlite::Result<Name<'a>> {
        fn is_valid<'a>(name: &'a OsStr, config: &Config) -> Name<'a> {
            let bytes = name.as_bytes();
            if bytes.starts_with(config.file_prefix().as_bytes()) {
                Name::File(OsStr::from_bytes(&bytes[1..]))
            } else if bytes.starts_with(config.tag_prefix().as_bytes()) {
                Name::Tag(OsStr::from_bytes(&bytes[1..]))
            } else {
                Name::None
            }
        }

        let check_db = |name: &Name| {
            let db = self.connect_db()?;
            debug!("SEARCHING DATABASE FOR {name:?}");
            let (query, value) = match name {
                Name::File(file) => ("select 1 from Files where file = ? limit 1", &file.to_string_lossy()),
                Name::Tag(tag) => ("select 1 from Tags where tag = ? limit 1", &tag.to_string_lossy()),
                Name::None => return Ok(false),
                Name::Root => unreachable!(),
            };
            let mut stmt = db.prepare_cached(query)?;
            stmt.exists([value])
        };
        
        if path == std::path::Path::new("/") {
            return Ok(Name::Root);
        }

        match path.file_name().map(|n| is_valid(n, &self.config)) {
            Some(name) if check_db(&name)? => Ok(name),
            _ => Ok(Name::None),
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

        let exists = ok_or_panic!(
            self.name_exists(path),
            "[getattr] database connection failed"
        );
        match exists {
            Name::File(_) => Ok((TTL, LINK_ATTR)),
            Name::Tag(_) | Name::Root => Ok((TTL, ROOT_DIR_ATTR)),
            Name::None => {
                debug!("[getattr] path {path:?} does not exist");
                Err(libc::ENOENT)
            }
        }
    }

    fn opendir(
        &self,
        _req: fuse_mt::RequestInfo,
        path: &std::path::Path,
        flags: u32,
    ) -> fuse_mt::ResultOpen {
        debug!("[opendir] path = {path:?}, flags = {flags:#o}");
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
        debug!("[readdir] path = {path:?}, fh = {fh}");

        let exists = ok_or_panic!(
            self.name_exists(path),
            "[readdir] database connection failed"
        );
        match exists {
            Name::Tag(_) => {}
            Name::Root => {
                info!("[readdir::sqlite] '/' tag query");
                let instant = std::time::Instant::now();
                
                let db = self.connect_db().unwrap();
                let mut stmt = db.prepare_cached(r#"
                    SELECT tag 
                    FROM Tags t
                    WHERE EXISTS (
                        SELECT 1 
                        FROM FileTags ft 
                        WHERE ft.tagId = t.id
                    )
                "#).unwrap();
                let entries = stmt.query_map([], |r| {
                    let name = format!("{}{}", self.config.tag_prefix(), r.get_ref(0)?.as_str()?).into();
                    Ok(fusemt::DirectoryEntry { name, kind: fusemt::FileType::Directory })
                })
                .unwrap()
                .flatten()
                .collect();
                info!("[readdir::sqlite] '/' tag query done in {:?}", instant.elapsed());
                return Ok(entries);
            }
            Name::File(_) => return Err(libc::ENOTDIR),
            Name::None => return Err(libc::ENOENT),
        }

        let tags = path
            .components()
            .skip(1)
            .flat_map(|c| std::str::from_utf8(c.as_os_str().as_bytes()))
            .map(|s| &s[1..]);

        let db = ok_or_panic!(
            self.connect_db(),
            "[readdir] failed to connect to sqlite database"
        );
        let tags_list = crate::utils::list_to_values(tags);
        // Commented in "/src/sql/db.sql"
        let stmt = format!(
            r#"
                WITH
                TargetTags AS ( VALUES {tags_list} ),
                TagsLen AS (
                    SELECT COUNT(*) AS len FROM TargetTags
                ),
                FoundFiles AS MATERIALIZED (
                    SELECT ft.fileId as id
                    FROM FileTags ft
                    JOIN Tags t ON t.id = ft.tagId
                    WHERE t.tag IN TargetTags
                    GROUP BY ft.fileId
                    HAVING COUNT(t.tag) = (SELECT len FROM TagsLen)
                ),
                FoundTags AS MATERIALIZED (
                    SELECT DISTINCT t.tag
                    FROM FileTags ft
                    JOIN Tags t ON t.id = ft.tagId
                    WHERE ft.fileId IN FoundFiles
                    AND t.tag NOT IN TargetTags
                )
                SELECT t.tag, NULL AS file
                FROM FoundTags t
                UNION ALL
                SELECT NULL AS tag, f.file
                FROM FoundFiles ff
                JOIN Files f ON f.id = ff.id
            "#
        );
        let mut prep_stmt = db.prepare_cached(&stmt).unwrap();
        let results = prep_stmt
            .query_map([], |r| {
                let tag = r.get_ref(0)?.as_str_or_null()?;
                if let Some(tag) = tag {
                    let prefix = self.config.tag_prefix();
                    let tag = format!("{prefix}{tag}");
                    let kind = fusemt::FileType::Directory;
                    Ok((tag, kind))
                } else {
                    let prefix = self.config.file_prefix();
                    let file = { 
                        let p = r.get_ref(1)?.as_str()?;
                        let start = p.rfind('/').unwrap_or_else(|| panic!("expected file name in {p}"));
                        &p[start+1..]
                    };
                    let kind = fusemt::FileType::Symlink;
                    Ok((format!("{prefix}{file}"), kind))
                }
            })
            .unwrap()
            .flatten()
            .map(|(s, t)| (OsString::from(s), t));

        let entries = [
            (std::ffi::OsString::from("."), fusemt::FileType::Directory),
            (std::ffi::OsString::from(".."), fusemt::FileType::Directory),
        ]
        .into_iter()
        .chain(results)
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
