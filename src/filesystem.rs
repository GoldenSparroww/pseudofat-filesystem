//! Filesystem pseudoFAT

mod dir;
mod disk;
mod fat;
mod paths;
mod superblock;

use std::ops::Range;
use std::path::Path;
use std::{fmt, io};

use crate::config::{CLUSTER_SIZE, MAX_DISK_SIZE};
use crate::filesystem::dir::{Dir, Name};
use crate::filesystem::disk::Disk;
use crate::filesystem::fat::{FAT, FATPtrEntry};
use crate::filesystem::superblock::Superblock;

// Reexport what commands might need (ls)
pub(crate) use dir::{DirEntry, DirEntryKind};

#[derive(Debug)]
pub struct FileSystem {
    disk: Disk,
    superblock: Superblock,
    fat: FAT,
    cwd: u32,
}

#[derive(Debug)]
pub enum FsError {
    NotFormatted,
    DiskIo(io::Error),
    Corrupted(String),
    SuperblockCannotCreate(String),
    InvalidFileName(String),
    PathNotFound,
    DiskFull,
    DirFull,
    FileExist,
    HostFileNotFound,
    HostFileReadGeneralError,
    HostFileExists,
    HostFileWriteGeneralError,
    FileNotFound,
}

impl From<io::Error> for FsError {
    fn from(e: io::Error) -> Self {
        FsError::DiskIo(e)
    }
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::NotFormatted => write!(f, "Disk is not formatted."),
            FsError::DiskIo(e) => write!(f, "Disk I/O error: {e}"),
            FsError::Corrupted(msg) => write!(f, "Filesystem corrupted: {msg}"),
            FsError::SuperblockCannotCreate(msg) => write!(f, "Cannot create superblock: {msg}"),
            FsError::InvalidFileName(detail) => write!(f, "Invalid file name: ({detail})"),
            FsError::PathNotFound => write!(f, "Path not found."),
            FsError::DiskFull => write!(f, "Disk is full."),
            FsError::DirFull => write!(f, "Directory is full."),
            FsError::FileExist => write!(f, "File exists."),
            FsError::HostFileNotFound => write!(f, "Host file not found."),
            FsError::HostFileReadGeneralError => write!(f, "An error occured during file read on host."),
            FsError::HostFileExists => write!(f, "File exists on host."),
            FsError::HostFileWriteGeneralError => write!(f, "An error occured during file write on host."),
            FsError::FileNotFound => write!(f, "FILE NOT FOUND"),
        }
    }
}

impl FileSystem {
    pub fn try_open(path: &Path) -> Result<FileSystem, FsError> {
        let disk = match Disk::open(path) {
            Ok(disk) => disk,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(FsError::NotFormatted),
            Err(e) => return Err(e.into()),
        };

        let superblock = Superblock::load(&disk)?;

        let fat = FAT::load(&disk, superblock.fat_start, superblock.cluster_count)?;

        let cwd = superblock.root_cluster;

        Ok(FileSystem { disk, superblock, fat, cwd })
    }

    pub fn try_format(path: &Path, size: u64) -> Result<FileSystem, FsError> {
        let mut disk = Disk::create(path, size)?;

        let superblock = Superblock::new(size, CLUSTER_SIZE)?;
        superblock.save(&mut disk)?;

        let mut fat = FAT::new(superblock.fat_start, superblock.cluster_count);
        fat.save(&mut disk)?;

        // Directories have FATPtrEntry::EOF in FAT and no size (zero / one cluster)
        fat.set(&mut disk, superblock.root_cluster, FATPtrEntry::EOF)?;
        Dir::create(&mut disk, &superblock, superblock.root_cluster, superblock.root_cluster)?;
        let cwd = superblock.root_cluster;

        Ok(FileSystem { disk, superblock, fat, cwd })
    }

    pub fn info_superblock(&self) -> String {
        self.superblock.to_string()
    }

    pub fn info_fat(&self) -> String {
        self.fat.to_string()
    }
}

// Access functions GENERAL
impl FileSystem {
    pub fn set_cwd(&mut self, cwd_cluster: u32) {
        self.cwd = cwd_cluster
    }

    pub fn get_cwd(&self) -> u32 {
        self.cwd
    }

    pub fn get_root_cluster(&self) -> u32 {
        self.superblock.root_cluster
    }

    pub fn get_cluster_size(&self) -> u32 {
        self.superblock.cluster_size
    }

    /// Moves `entry` from `parent` into `target` under the name `name`. Data is not copied.
    pub fn move_entry(
        &mut self,
        parent_dir: &mut Dir,
        entry: &DirEntry,
        target_dir: &mut Dir,
        name: Name,
    ) -> Result<(), FsError> {
        let new_entry = DirEntry { name, ..*entry };

        // Rename in the same directory: overwrite the slot (works even if the directory is full).
        if parent_dir.get_cluster() == target_dir.get_cluster() {
            return parent_dir.replace(&mut self.disk, &entry.name, new_entry);
        }

        // When moving a directory, rewrite the `..` so that it points to the new parent. file => do nothing
        // Keep moved_dir data cluster to rewire its .. later
        let moved_dir = match entry.kind {
            DirEntryKind::Dir => Some(
                self.get_dir_cluster(entry)
                    .ok_or_else(|| FsError::Corrupted(format!("directory {} has no cluster", entry.name)))?,
            ),
            DirEntryKind::File { .. } => None,
        };

        // Order: new entry, removal of the old one, `..` last.
        target_dir.insert(&mut self.disk, new_entry)?;
        parent_dir.remove(&mut self.disk, &entry.name)?;

        // Perform a rewire of ..
        if let Some(cluster) = moved_dir {
            let new_dotdot = DirEntry::dir(Name::DOTDOT, target_dir.get_cluster());
            self.get_dir(cluster)?.replace(&mut self.disk, &Name::DOTDOT, new_dotdot)?;
        }

        Ok(())
    }
}

// Access functions DIR
impl FileSystem {
    pub fn get_dir(&self, cluster: u32) -> Result<Dir, FsError> {
        Dir::get(&self.disk, &self.superblock, cluster)
    }

    pub fn create_dir(&mut self, parent: &mut Dir, name: Name) -> Result<(), FsError> {
        // Create dir: check -> FAT (allocate) -> new cluster (`.`, `..`) -> entry in parent (insert + save slot).
        parent.is_insert_possible(&name)?;
        let cluster = self.fat.allocate(&mut self.disk, 1)?[0];
        Dir::create(&mut self.disk, &self.superblock, cluster, parent.get_cluster())?;
        parent.insert(&mut self.disk, DirEntry::dir(name, cluster))
    }

    pub fn remove_dir(&mut self, cluster: u32) -> Result<(), FsError> {
        // Remove dir: entry in parent (remove + save slot) -> FAT (free chain).
        let (mut parent, entry) = self.dir_of(cluster)?;
        parent.remove(&mut self.disk, &entry.name)?;
        self.fat.free_chain(&mut self.disk, entry.start)
    }

    /// Cluster of a directory; `None` for a file (or a corrupted directory entry)
    pub fn get_dir_cluster(&self, entry: &DirEntry) -> Option<u32> {
        match (entry.kind, entry.start) {
            (DirEntryKind::Dir, FATPtrEntry::Next(cluster)) => Some(cluster),
            _ => None,
        }
    }
}

// Access functions FILE
impl FileSystem {
    pub fn create_file(&mut self, parent_dir: &mut Dir, name: Name, data: &[u8]) -> Result<(), FsError> {
        if data.len() as u64 > MAX_DISK_SIZE {
            return Err(FsError::DiskFull);
        }
        let size = u32::try_from(data.len()).map_err(|_| FsError::DiskFull)?;

        // Order of entries: dir full?, FAT, data, and finally the directory entry.

        parent_dir.is_insert_possible(&name)?;

        let count = size.div_ceil(CLUSTER_SIZE);
        let clusters = self.fat.allocate(&mut self.disk, count as usize)?;

        for (offset, range) in self.file_to_extents(&clusters, data.len()) {
            self.disk.write_at(offset, &data[range])?;
        }

        let start = match clusters.first() {
            Some(&first) => FATPtrEntry::Next(first),
            // An empty file has no clusters
            None => FATPtrEntry::EOF,
        };

        parent_dir.insert(&mut self.disk, DirEntry::file(name, size, start))
    }

    pub fn read_file(&self, entry: &DirEntry) -> Result<Vec<u8>, FsError> {
        let DirEntryKind::File { size } = entry.kind else {
            return Err(FsError::FileNotFound);
        };

        let clusters = self.fat.get_chain(entry.start)?;
        let needed = (size as usize).div_ceil(self.superblock.cluster_size as usize);
        if clusters.len() != needed {
            return Err(FsError::Corrupted(format!(
                "size {size} B needs {needed} clusters, chain has {}",
                clusters.len()
            )));
        }

        let mut data = vec![0u8; size as usize];
        for (offset, range) in self.file_to_extents(&clusters, data.len()) {
            self.disk.read_at(offset, &mut data[range])?;
        }
        Ok(data)
    }

    pub fn remove_file(&mut self, parent: &mut Dir, entry: &DirEntry) -> Result<(), FsError> {
        // Validate the chain first, free_chain could fail only after the entry is already gone
        self.fat.get_chain(entry.start)?;

        // Order: entry first, then FAT.
        parent.remove(&mut self.disk, &entry.name)?;
        self.fat.free_chain(&mut self.disk, entry.start)
    }

    pub fn get_file_clusters(&self, entry: &DirEntry) -> Result<Vec<u32>, FsError> {
        self.fat.get_chain(entry.start)
    }
}

// Inner helpers
impl FileSystem {
    /// Groups chain of clusters into (disk_offset, data_range) extents, clamping the last range to size.
    /// e.g. with cluster_size = 100, [4, 5, 9] and size = 250 give [(offset(4), 0..200), (offset(9), 200..250)].
    fn file_to_extents(&self, clusters: &[u32], size: usize) -> Vec<(u64, Range<usize>)> {
        let cluster_size = self.superblock.cluster_size as usize;
        let mut extents: Vec<(u64, Range<usize>)> = Vec::new();
        let mut start = 0;

        for contiguous in clusters.chunk_by(|&a, &b| b == a + 1) {
            // min() ensures perfect byte alignment for last cluster for caller's indexing in data bytes
            let end = (start + contiguous.len() * cluster_size).min(size);
            extents.push((self.superblock.cluster_real_byte_offset(contiguous[0]), start..end));
            start = end;
        }

        extents
    }
}
