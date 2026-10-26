//! Filesystem pseudoFAT

mod dir;
mod disk;
mod fat;
mod superblock;

use std::path::Path;
use std::{fmt, io};

use crate::config::CLUSTER_SIZE;
use crate::filesystem::dir::Dir;
use crate::filesystem::disk::Disk;
use crate::filesystem::fat::{FAT, FATPtrEntry};
use crate::filesystem::superblock::Superblock;

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
        }
    }
}

impl FileSystem {
    pub fn try_open(path: &Path) -> Result<FileSystem, FsError> {
        let mut disk = match Disk::open(path) {
            Ok(disk) => disk,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(FsError::NotFormatted),
            Err(e) => return Err(e.into()),
        };

        let superblock = Superblock::load(&mut disk)?;

        let fat = FAT::load(&mut disk, superblock.fat_start, superblock.cluster_count)?;

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
        Dir::create_dir(&mut disk, &superblock, superblock.root_cluster, superblock.root_cluster)?;
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
