use std::time::UNIX_EPOCH;

use fuser::Filesystem;
use libc::ENOENT;

const TTL: std::time::Duration = std::time::Duration::from_secs(1); // 1 second

const ROOT_DIR_ATTR: fuser::FileAttr = fuser::FileAttr {
    ino: 1,
    size: 0,
    blocks: 0,
    atime: UNIX_EPOCH, // 1970-01-01 00:00:00
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    crtime: UNIX_EPOCH,
    kind: fuser::FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 501,
    gid: 20,
    rdev: 0,
    flags: 0,
    blksize: 512,
};

const PASTA_DIR_ATTR: fuser::FileAttr = const { 
    fuser::FileAttr {
        ino: 2,
        ..ROOT_DIR_ATTR
    }
};

const LINK_ATTR: fuser::FileAttr = const {
    fuser::FileAttr {
        ino: 3,
        kind: fuser::FileType::Symlink,
        ..ROOT_DIR_ATTR
    }
};

pub struct Fuse;

impl Filesystem for Fuse {
    fn lookup(&mut self, _req: &fuser::Request<'_>, parent: u64, name: &std::ffi::OsStr, reply: fuser::ReplyEntry) {
        if parent == 1 && name.to_str() == Some("link") {
            reply.entry(&TTL, &LINK_ATTR, 0)
        } else {
            reply.error(ENOENT)
        }
    }
    fn getattr(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyAttr) {
        match ino {
            1 => reply.attr(&TTL, &ROOT_DIR_ATTR),
            2 => reply.attr(&TTL, &PASTA_DIR_ATTR),
            3 => reply.attr(&TTL, &LINK_ATTR),
            _ => reply.error(ENOENT)
        }
    }
    fn readlink(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyData) {
        if ino == 3 {
            reply.data(b"../target");
        } else {
            reply.error(ENOENT);
        }
    }
    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        if ino != 1 {
            return reply.error(ENOENT);
        }

        let entries = [
            (1, fuser::FileType::Directory, "."),
            (1, fuser::FileType::Directory, ".."),
            (2, fuser::FileType::Directory, "pasta"),
            (3, fuser::FileType::Symlink, "link"),
        ];

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            // i + 1 means the index of the next entry
            if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                break;
            }
        }
        reply.ok();
    }
}
