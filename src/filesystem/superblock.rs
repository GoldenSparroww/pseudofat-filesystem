//! Superblock
//!
//! Also called Volume Boot Record (VBR), Boot Sector, BIOS Parameter Block (BPB)

use std::fmt;

use crate::{
    bytes,
    config::{MAX_DISK_SIZE, MIB, MIN_CLUSTER_SIZE},
    filesystem::{
        FsError::{self, Corrupted, SuperblockCannotCreate},
        disk::Disk,
        fat,
    },
};

pub const SUPERBLOCK_SIZE: usize = 64;
const AT_MAGIC: usize = 0;
const AT_VERSION: usize = 8;
const AT_CLUSTER_SIZE: usize = 12;
const AT_CLUSTER_COUNT: usize = 16;
const AT_ROOT_CLUSTER: usize = 20;
const AT_DISK_SIZE: usize = 24;
const AT_FAT_START: usize = 32;
const AT_DATA_START: usize = 40;
// Padding until 63

const MAGIC: [u8; 8] = *b"PSEUDFAT";
const VERSION: u32 = 1;

#[derive(Debug, PartialEq, Eq)]
pub struct Superblock {
    magic: String,
    version: u32,
    pub cluster_size: u32,
    pub cluster_count: u32,
    pub root_cluster: u32,
    pub disk_size: u64,
    pub fat_start: u64,
    pub data_start: u64,
}

impl Superblock {
    pub fn new(disk_size: u64, cluster_size: u32) -> Result<Superblock, FsError> {
        if cluster_size < MIN_CLUSTER_SIZE {
            return Err(SuperblockCannotCreate(format!(
                "Cluster size too small (minimum allowed: {} B).",
                MIN_CLUSTER_SIZE
            )));
        }

        if disk_size > MAX_DISK_SIZE {
            return Err(SuperblockCannotCreate(format!(
                "Disk too large (maximum allowed: {} MB).",
                MAX_DISK_SIZE / MIB
            )));
        }

        if !cluster_size.is_power_of_two() {
            return Err(SuperblockCannotCreate(format!(
                "Cluster size should be a power of two ({cluster_size} B given)."
            )));
        }

        let fat_start = SUPERBLOCK_SIZE as u64;

        let bytes_per_cluster = u64::from(cluster_size) + fat::FAT_INDEX_SIZE;
        // saturating_sub: disk_size - fat_start WILL BE >= 0; zero if disk is too small
        let count = disk_size.saturating_sub(fat_start) / bytes_per_cluster;

        // Can our FAT address clusters we want?
        let cluster_count = match u32::try_from(count) {
            // Zero from previous step (count = ... / ...)
            Ok(0) => {
                return Err(SuperblockCannotCreate(format!(
                    "Disk too small. Condition: Disk size >= Superblock + one FAT index + one cluster. ({} >= {} + {} + {})",
                    disk_size,
                    SUPERBLOCK_SIZE,
                    fat::FAT_INDEX_SIZE,
                    cluster_size
                )));
            }
            // Not only respect u32::try_from but our stricter ones too
            Ok(n) if n <= fat::MAX_CLUSTER_COUNT => n,
            _ => {
                return Err(SuperblockCannotCreate(
                    "Disk too large. Icrease cluster size, or lower disk size.".to_string(),
                ));
            }
        };

        Ok(Superblock {
            magic: String::from_utf8_lossy(&MAGIC).into_owned(),
            version: VERSION,
            cluster_size,
            cluster_count,
            root_cluster: 0,
            disk_size,
            fat_start,
            data_start: fat_start + count * fat::FAT_INDEX_SIZE,
        })
    }

    pub fn load(disk: &Disk) -> Result<Superblock, FsError> {
        let disk_size = disk.get_size()?;

        // Can I at least read superblock?
        if disk_size < SUPERBLOCK_SIZE as u64 {
            return Err(FsError::NotFormatted);
        }

        let mut bytes = [0u8; SUPERBLOCK_SIZE];
        disk.read_at(0, &mut bytes)?;

        Superblock::from_bytes(&bytes, disk_size)
    }

    pub fn save(&self, disk: &mut Disk) -> Result<(), FsError> {
        disk.write_at(0, &self.to_bytes())
    }

    fn from_bytes(bytes: &[u8; SUPERBLOCK_SIZE], disk_size: u64) -> Result<Superblock, FsError> {
        if bytes::get_bytes_seq(bytes, AT_MAGIC) != MAGIC {
            return Err(FsError::NotFormatted);
        }

        if bytes::get_u32(bytes, AT_VERSION) != VERSION {
            return Err(Corrupted(String::from("unsupported version")));
        }

        let superblock = Superblock {
            magic: String::from_utf8_lossy(&MAGIC).into_owned(),
            version: VERSION,
            cluster_size: bytes::get_u32(bytes, AT_CLUSTER_SIZE),
            cluster_count: bytes::get_u32(bytes, AT_CLUSTER_COUNT),
            root_cluster: bytes::get_u32(bytes, AT_ROOT_CLUSTER),
            disk_size: bytes::get_u64(bytes, AT_DISK_SIZE),
            fat_start: bytes::get_u64(bytes, AT_FAT_START),
            data_start: bytes::get_u64(bytes, AT_DATA_START),
        };

        if superblock.disk_size != disk_size {
            return Err(Corrupted(format!("superblock says {} B, file has {disk_size} B", superblock.disk_size)));
        }

        // rest must align perfecly under same conditions, when another is created with same values
        let expected = Superblock::new(superblock.disk_size, superblock.cluster_size)?;
        if superblock != expected {
            return Err(Corrupted(String::from("inconsistent layout")));
        }

        Ok(superblock)
    }

    fn to_bytes(&self) -> [u8; SUPERBLOCK_SIZE] {
        let mut bytes = [0u8; SUPERBLOCK_SIZE];

        bytes::put_bytes_seq(&mut bytes, AT_MAGIC, &MAGIC);
        bytes::put_u32(&mut bytes, AT_VERSION, VERSION);
        bytes::put_u32(&mut bytes, AT_CLUSTER_SIZE, self.cluster_size);
        bytes::put_u32(&mut bytes, AT_CLUSTER_COUNT, self.cluster_count);
        bytes::put_u32(&mut bytes, AT_ROOT_CLUSTER, self.root_cluster);
        bytes::put_u64(&mut bytes, AT_DISK_SIZE, self.disk_size);
        bytes::put_u64(&mut bytes, AT_FAT_START, self.fat_start);
        bytes::put_u64(&mut bytes, AT_DATA_START, self.data_start);

        bytes
    }

    pub fn cluster_real_byte_offset(&self, cluster: u32) -> u64 {
        // Don't forget, clusters are just terms within data; self.data_start + ...
        self.data_start + u64::from(cluster) * u64::from(self.cluster_size)
    }
}

impl fmt::Display for Superblock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Disk size: {} B, {} clusters X {} B, FAT at {}, data at {}",
            self.disk_size, self.cluster_count, self.cluster_size, self.fat_start, self.data_start
        )
    }
}
