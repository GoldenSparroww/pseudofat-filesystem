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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub fn load(disk: &Disk, fat_start: u64, cluster_count: u32) -> Result<FAT, FsError> {
        let mut bytes = vec![0u8; (cluster_count as usize) * (FAT_INDEX_SIZE as usize)];
        disk.read_at(fat_start, &mut bytes)
            .map_err(|e| Corrupted(format!("Cannot read FAT table. ({})", e)))?;

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

    /// Item for the `cluster` cluster; `None` for a number outside the range
    pub fn get(&self, cluster: u32) -> Option<FATPtrEntry> {
        // It doesn't directly index entries[i], because 300,000 entries but only have 100,000 clusters
        self.ptr_entries.get(cluster as usize).map(|&raw| FATPtrEntry::from_raw(raw))
    }

    /// Save entries `first..=last`. Abstracts out FAT offset
    fn save_range(&self, disk: &mut Disk, first: usize, last: usize) -> Result<(), FsError> {
        let bytes: Vec<u8> = self.ptr_entries[first..=last]
            .iter()
            .flat_map(|raw| raw.to_le_bytes())
            .collect();
        disk.write_at(self.fat_start + (first as u64) * FAT_INDEX_SIZE, &bytes)
    }

    /// Finds `count` available clusters and concatenates them into a string ending with EOF.
    /// Returns the cluster numbers in the order they appear in the string
    pub fn allocate(&mut self, disk: &mut Disk, count: usize) -> Result<Vec<u32>, FsError> {
        let clusters: Vec<u32> = (0..(self.ptr_entries.len() as u32))
            .filter(|&cluster| self.ptr_entries[cluster as usize] == RAW_FREE)
            .take(count)
            .collect();

        if clusters.len() < count {
            return Err(FsError::DiskFull);
        }

        // Each cluster points at next one, last is EOF
        for (i, &cluster) in clusters.iter().enumerate() {
            let entry = match clusters.get(i + 1) {
                Some(&next) => FATPtrEntry::Next(next),
                None => FATPtrEntry::EOF,
            };
            self.ptr_entries[cluster as usize] = entry.to_raw();
        }

        // Nothing to write when `count == 0` (clusters is empty)
        if let (Some(&first), Some(&last)) = (clusters.iter().min(), clusters.iter().max()) {
            self.save_range(disk, first as usize, last as usize)?;
        }

        Ok(clusters)
    }

    /// Iterates through the string starting at `first` and returns the numbers of its clusters in order
    /// <br> **NOTE**: I have to pass a sort of “artificial” reference to `first`, i.e., `FATEntry::Next(first)`, otherwise
    /// the first cluster would be omitted.
    pub fn get_chain(&self, first: FATPtrEntry) -> Result<Vec<u32>, FsError> {
        let mut clusters = Vec::new();
        let mut current = first;

        loop {
            let cluster = match current {
                FATPtrEntry::EOF => return Ok(clusters),
                FATPtrEntry::Next(cluster) => cluster,
                FATPtrEntry::FREE => return Err(Corrupted("chain contains a FREE entry".to_owned())),
                FATPtrEntry::BAD => return Err(Corrupted("chain contains a BAD entry".to_owned())),
            };

            if clusters.len() >= self.ptr_entries.len() {
                return Err(Corrupted("cycle in chain".to_owned()));
            }

            let Some(next) = self.get(cluster) else {
                return Err(Corrupted(format!("invalid cluster number {cluster}")));
            };

            clusters.push(cluster);
            current = next;
        }
    }

    pub fn free_chain(&mut self, disk: &mut Disk, first: FATPtrEntry) -> Result<(), FsError> {
        let chain_of_clusters = self.get_chain(first)?;

        for &cluster in &chain_of_clusters {
            self.ptr_entries[cluster as usize] = RAW_FREE;
        }

        if let (Some(&first), Some(&last)) = (chain_of_clusters.iter().min(), chain_of_clusters.iter().max()) {
            self.save_range(disk, first as usize, last as usize)?;
        }

        Ok(())
    }
}

impl fmt::Display for FAT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Free clusters: {}", self.ptr_entries.iter().filter(|&&raw| raw == RAW_FREE).count() as u32)
    }
}
