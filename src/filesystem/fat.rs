//! FAT

/// Velikost jedné položky FAT na disku v bajtech (`u32`)
pub const FAT_INDEX_SIZE: u64 = 4;

/// Tři nejvyšší hodnoty `u32` jsou vyhrazené pro stavy FREE, EOF a BAD
pub const MAX_CLUSTER_COUNT: u32 = u32::MAX - 2;
