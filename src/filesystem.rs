//! Filesystem pseudoFAT

mod dir;
mod disk;
mod fat;
mod paths;
mod superblock;

use std::path::Path;
use std::{fmt, io};

use crate::config::CLUSTER_SIZE;
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

// Access functions
impl FileSystem {
    pub fn set_cwd(&mut self, cwd_cluster: u32) {
        self.cwd = cwd_cluster
    }

    pub fn get_cwd(&self) -> u32 {
        self.cwd
    }

    pub fn get_root(&self) -> u32 {
        self.superblock.root_cluster
    }

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
}
