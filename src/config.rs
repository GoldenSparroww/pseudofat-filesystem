//! Configurable constants of pseudoFAT

pub const KIB: u64 = 1024;
pub const MIB: u64 = 1024 * KIB;
pub const GIB: u64 = 1024 * MIB;

pub const CLUSTER_SIZE: u32 = 1024;

// Artificial/realistic
pub const MIN_CLUSTER_SIZE: u32 = 512;

// Artificial
pub const MAX_DISK_SIZE: u64 = 4 * GIB;
