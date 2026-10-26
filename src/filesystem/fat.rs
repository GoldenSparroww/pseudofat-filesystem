//! FAT - File Allocation Table

use std::fmt;

use crate::filesystem::{
    FsError::{self, Corrupted},
    disk::Disk,
};

pub const FAT_INDEX_SIZE: u64 = 4;

pub const MAX_CLUSTER_COUNT: u32 = u32::MAX - 2;

const RAW_FREE: u32 = u32::MAX;
const RAW_EOF: u32 = u32::MAX - 1;
const RAW_BAD: u32 = u32::MAX - 2;

#[derive(Debug, Clone, Copy)]
pub enum FATPtrEntry {
    FREE,
    EOF,
    BAD,
    Next(u32),
}

impl FATPtrEntry {
    pub fn from_raw(raw: u32) -> FATPtrEntry {
        match raw {
            RAW_FREE => FATPtrEntry::FREE,
            RAW_EOF => FATPtrEntry::EOF,
            RAW_BAD => FATPtrEntry::BAD,
            next => FATPtrEntry::Next(next),
        }
    }

    pub fn to_raw(self) -> u32 {
        match self {
            FATPtrEntry::FREE => RAW_FREE,
            FATPtrEntry::EOF => RAW_EOF,
            FATPtrEntry::BAD => RAW_BAD,
            FATPtrEntry::Next(next) => next,
        }
    }
}

#[derive(Debug)]
pub struct FAT {
    fat_start: u64,
    ptr_entries: Vec<u32>,
}

impl FAT {
    pub fn new(fat_start: u64, cluster_count: u32) -> FAT {
        // Have to be filled with RAW_FREE, not 0, it would mean referring to cluster 0 otherwise
        FAT { fat_start, ptr_entries: vec![RAW_FREE; cluster_count as usize] }
    }

    pub fn load(disk: &mut Disk, fat_start: u64, cluster_count: u32) -> Result<FAT, FsError> {
        let mut bytes = vec![0u8; (cluster_count as usize) * (FAT_INDEX_SIZE as usize)];
        disk.read_at(fat_start, &mut bytes)
            .map_err(|e| Corrupted(format!("Cannot read FAT table. ({})", e)));

        // as_chunks::<4>() divides to an array [u8; 4] (each chunk is one pointer/entry)
        // rest is always empty - ensured in Superblock::new()
        let (chunks, _rest) = bytes.as_chunks::<{ FAT_INDEX_SIZE as usize }>();
        let ptr_entries = chunks.iter().map(|&chunk| u32::from_le_bytes(chunk)).collect();

        Ok(FAT { fat_start, ptr_entries })
    }

    /// Safe whole FAT on disk
    pub fn save(&self, disk: &mut Disk) -> Result<(), FsError> {
        match self.ptr_entries.len() {
            0 => Ok(()),
            len => self.save_range(disk, 0, len - 1),
        }
    }

    // Set just one cluster
    pub fn set(&mut self, disk: &mut Disk, cluster_as_index: u32, ptr_entry: FATPtrEntry) -> Result<(), FsError> {
        let index = cluster_as_index as usize;
        self.ptr_entries[index] = ptr_entry.to_raw();
        self.save_range(disk, index, index)
    }

    /// Save entries `first..=last`. Abstracts out FAT offset
    fn save_range(&self, disk: &mut Disk, first: usize, last: usize) -> Result<(), FsError> {
        let bytes: Vec<u8> = self.ptr_entries[first..=last]
            .iter()
            .flat_map(|raw| raw.to_le_bytes())
            .collect();
        disk.write_at(self.fat_start + (first as u64) * FAT_INDEX_SIZE, &bytes)
    }
}

impl fmt::Display for FAT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Free clusters: {}", self.ptr_entries.iter().filter(|&&raw| raw == RAW_FREE).count() as u32)
    }
}
