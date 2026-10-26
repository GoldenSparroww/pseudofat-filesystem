use crate::{
    bytes,
    filesystem::{FsError, disk::Disk, fat::FATPtrEntry, superblock::Superblock},
};

pub const DIR_ENTRY_SIZE: usize = 32;

const NAME_SIZE: usize = 12;
// const BASE_LEN: usize = 8;
// const EXT_LEN: usize = 3;

const AT_NAME: usize = 0;
const AT_KIND: usize = 12;
const AT_SIZE: usize = 16;
const AT_START: usize = 20;
// Padding until 31

// const KIND_FREE: u8 = 0;
const KIND_FILE: u8 = 1;
const KIND_DIR: u8 = 2;

// ===== DirEntry ======

pub struct Name([u8; NAME_SIZE]);

impl Name {
    pub const DOT: Name = Name(*b".\0\0\0\0\0\0\0\0\0\0\0");
    pub const DOTDOT: Name = Name(*b"..\0\0\0\0\0\0\0\0\0\0");
}

pub enum DirEntryKind {
    File { size: u32 },
    Dir,
}

pub struct DirEntry {
    pub name: Name,
    pub kind: DirEntryKind,
    pub start: FATPtrEntry,
}

impl DirEntry {
    pub fn dir(name: Name, cluster: u32) -> DirEntry {
        // Cluster needs to be wrapped into FATPtrEntry::Next for FAT functions
        DirEntry { name, kind: DirEntryKind::Dir, start: FATPtrEntry::Next(cluster) }
    }

    /*pub fn file(name: Name, size: u32, start: FATPtrEntry) -> DirEntry {
        DirEntry { name, kind: DirEntryKind::File { size }, start }
    }*/

    fn to_bytes(&self) -> [u8; DIR_ENTRY_SIZE] {
        let mut bytes = [0u8; DIR_ENTRY_SIZE];

        let kind = match self.kind {
            DirEntryKind::File { size: _ } => KIND_FILE,
            DirEntryKind::Dir => KIND_DIR,
        };

        let size = match self.kind {
            DirEntryKind::File { size } => size,
            DirEntryKind::Dir => 0,
        };

        bytes::put_bytes_seq(&mut bytes, AT_NAME, &self.name.0);
        bytes::put_u32(&mut bytes, AT_KIND, kind.into());
        bytes::put_u32(&mut bytes, AT_SIZE, size);
        bytes::put_u32(&mut bytes, AT_START, self.start.to_raw());
        bytes
    }
}

// ===== Dir ======

pub struct Dir {
    cluster: u32,
    real_cluster_bytes_offset: u64,
    slots: Vec<Option<DirEntry>>,
}

impl Dir {
    /// Writes to `cluster` new empty directory with `.`, `..`.
    /// It is expeced, to cluster be present in FAT already, with propper attributes (FAT.ptr_entries[cluster] = XYZ) (= EOF e.g.)
    pub fn create_dir(disk: &mut Disk, superblock: &Superblock, cluster: u32, parent: u32) -> Result<(), FsError> {
        let mut bytes = vec![0u8; superblock.cluster_size as usize];

        // 0x00: .  (0) first record
        // 0x04: .. (DIR_ENTRY_SIZE = 0 + 4) second record
        bytes::put_bytes_seq(&mut bytes, 0, &DirEntry::dir(Name::DOT, cluster).to_bytes());
        bytes::put_bytes_seq(&mut bytes, DIR_ENTRY_SIZE, &DirEntry::dir(Name::DOTDOT, parent).to_bytes());

        disk.write_at(superblock.cluster_real_byte_offset(cluster), &bytes)
    }
}
