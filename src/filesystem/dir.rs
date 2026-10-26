use std::fmt;

use crate::{
    bytes::{self},
    filesystem::{FsError, disk::Disk, fat::FATPtrEntry, superblock::Superblock},
};

pub const DIR_ENTRY_SIZE: usize = 32;

const NAME_SIZE: usize = 12;
const BASE_LEN: usize = 8;
const EXT_LEN: usize = 3;

const AT_NAME: usize = 0;
const AT_KIND: usize = 12;
const AT_SIZE: usize = 16;
const AT_START: usize = 20;
/// Padding until 31

const KIND_FREE: u8 = 0; // Only used for reading dir from disk (distinguishes between emptiness and corrupted slots)
const KIND_FILE: u8 = 1;
const KIND_DIR: u8 = 2;

// ===== DirEntry: Name ======

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct Name([u8; NAME_SIZE]);

impl Name {
    pub const DOT: Name = Name(*b".\0\0\0\0\0\0\0\0\0\0\0");
    pub const DOTDOT: Name = Name(*b"..\0\0\0\0\0\0\0\0\0\0");

    pub fn from_str(text: &str) -> Result<Name, FsError> {
        let invalid = |reason: &str| FsError::InvalidFileName(format!("{text}: {reason}"));

        // just ASCI
        if let Some(c) = text.chars().find(|&c| !c.is_ascii_graphic() || c == '/') {
            return Err(invalid(&format!("invalid character {c:?}")));
        }

        let (base, ext) = match text.split_once('.') {
            None => (text, ""),
            Some((_, "")) => return Err(invalid("empty extension")),
            Some(parts) => parts,
        };

        if base.is_empty() {
            return Err(invalid("empty name"));
        }
        if ext.contains('.') {
            return Err(invalid("more than one dot"));
        }
        if base.len() > BASE_LEN {
            return Err(invalid("name longer than 8 characters"));
        }
        if ext.len() > EXT_LEN {
            return Err(invalid("extension longer than 3 characters"));
        }

        let mut bytes = [0u8; NAME_SIZE];
        bytes::put_bytes_seq(&mut bytes, 0, base.as_bytes());
        bytes::put_bytes_seq(&mut bytes, BASE_LEN, ext.as_bytes());
        Ok(Name(bytes))
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn until_null(bytes: &[u8]) -> &[u8] {
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            &bytes[..end]
        }

        let base = until_null(&self.0[..BASE_LEN]);
        let ext = until_null(&self.0[BASE_LEN..BASE_LEN + EXT_LEN]);

        // lossy: jméno z poškozeného disku nemusí být platné ASCII/UTF-8
        write!(f, "{}", String::from_utf8_lossy(base))?;
        if !ext.is_empty() {
            write!(f, ".{}", String::from_utf8_lossy(ext))?;
        }
        Ok(())
    }
}

// ===== DirEntry ======

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum DirEntryKind {
    File { size: u32 },
    Dir,
}

#[derive(Clone, Copy)]
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

    fn from_bytes(bytes: &[u8; DIR_ENTRY_SIZE]) -> Result<Option<DirEntry>, u8> {
        let kind = match bytes[AT_KIND] {
            KIND_FREE => return Ok(None),
            KIND_FILE => DirEntryKind::File { size: bytes::get_u32(bytes, AT_SIZE) },
            KIND_DIR => DirEntryKind::Dir,
            unknown => return Err(unknown),
        };

        Ok(Some(DirEntry {
            name: Name(bytes::get_bytes_seq(bytes, AT_NAME)),
            kind,
            start: FATPtrEntry::from_raw(bytes::get_u32(bytes, AT_START)),
        }))
    }
}

// ===== Dir ======

pub struct Dir {
    cluster: u32,
    real_cluster_bytes_offset: u64,
    slots: Vec<Option<DirEntry>>,
}

impl Dir {
    /// It is expeced, to cluster be present in FAT already, with propper attributes (FAT.ptr_entries[cluster] = XYZ) (= EOF e.g.)
    pub fn create(disk: &mut Disk, superblock: &Superblock, cluster: u32, parent: u32) -> Result<(), FsError> {
        let mut bytes = vec![0u8; superblock.cluster_size as usize];

        // 0x00: .  (0) first record
        // 0x04: .. (DIR_ENTRY_SIZE = 0 + 4) second record
        bytes::put_bytes_seq(&mut bytes, 0, &DirEntry::dir(Name::DOT, cluster).to_bytes());
        bytes::put_bytes_seq(&mut bytes, DIR_ENTRY_SIZE, &DirEntry::dir(Name::DOTDOT, parent).to_bytes());

        disk.write_at(superblock.cluster_real_byte_offset(cluster), &bytes)
    }

    pub fn get(disk: &Disk, superblock: &Superblock, cluster: u32) -> Result<Dir, FsError> {
        // Sanity check, is it smaller than the max?
        if cluster >= superblock.cluster_count {
            return Err(FsError::Corrupted(format!("directory cluster {cluster} out of range")));
        }

        let real_cluster_bytes_offset = superblock.cluster_real_byte_offset(cluster);
        let mut bytes = vec![0u8; superblock.cluster_size as usize];

        disk.read_at(real_cluster_bytes_offset, &mut bytes)?;

        let (chunks, _rest) = bytes.as_chunks::<DIR_ENTRY_SIZE>();
        let mut slots = Vec::new();

        for (index, chunk) in chunks.iter().enumerate() {
            let slot = DirEntry::from_bytes(chunk).map_err(|kind| {
                FsError::Corrupted(format!("directory cluster {cluster}, slot {index}: unknown kind {kind}"))
            })?;
            slots.push(slot);
        }

        Ok(Dir { cluster, real_cluster_bytes_offset, slots })
    }

    pub fn find(&self, name: &Name) -> Option<&DirEntry> {
        self.slots.iter().flatten().find(|entry| entry.name == *name)
    }

    /// Checks whether an entry with `name` can be inserted (name is unique, free slot exists).
    /// Returns index of the first free slot. Call it before allocating clusters, so nothing leaks on error
    pub fn check_insert(&self, name: &Name) -> Result<usize, FsError> {
        if self.find(name).is_some() {
            return Err(FsError::FileExist);
        }

        self.slots.iter().position(Option::is_none).ok_or(FsError::DirFull)
    }

    pub fn insert(&mut self, disk: &mut Disk, entry: DirEntry) -> Result<(), FsError> {
        let index = self.check_insert(&entry.name)?;

        self.slots[index] = Some(entry);
        self.save_slot(disk, index)
    }

    /// Clears the slot with `name` and writes it to disk. Does not touch FAT (caller frees the chain)
    pub fn remove(&mut self, disk: &mut Disk, name: &Name) -> Result<(), FsError> {
        let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|entry| entry.name == *name))
        else {
            return Err(FsError::PathNotFound);
        };

        self.slots[index] = None;
        self.save_slot(disk, index)
    }
}

impl Dir {
    pub fn get_cluster(&self) -> u32 {
        self.cluster
    }

    pub fn get_slots(&self) -> impl Iterator<Item = &DirEntry> {
        self.slots
            .iter()
            .flatten()
            .filter(|entry| entry.name != Name::DOT && entry.name != Name::DOTDOT)
    }

    fn save_slot(&self, disk: &mut Disk, index: usize) -> Result<(), FsError> {
        let bytes = match &self.slots[index] {
            Some(entry) => entry.to_bytes(),
            None => [0u8; DIR_ENTRY_SIZE],
        };

        disk.write_at(self.real_cluster_bytes_offset + (index * DIR_ENTRY_SIZE) as u64, &bytes)
    }
}
