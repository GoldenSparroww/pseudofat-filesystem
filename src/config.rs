//! Konfigurovatelné parametry pseudoFAT

// ---------------------------------------------------------------------------
// Jednotky (binární, mocniny 2)
// ---------------------------------------------------------------------------

/// Kibibajt: 1 KiB = 1024 B.
pub const KIB: u64 = 1024;

/// Mebibajt: 1 MiB = 1024 KiB = 1 048 576 B.
pub const MIB: u64 = 1024 * KIB;

/// Gibibajt: 1 GiB = 1024 MiB = 1 073 741 824 B.
pub const GIB: u64 = 1024 * MIB;

// ---------------------------------------------------------------------------
// Příkaz format
// ---------------------------------------------------------------------------

/// Počet bajtů, které představuje jednotka „MB“ v příkazu `format 600MB`.
pub const FORMAT_UNIT: u64 = MIB;

// ---------------------------------------------------------------------------
// Souborový systém
// ---------------------------------------------------------------------------

/// Velikost clusteru v bajtech
pub const CLUSTER_SIZE: u32 = 1024;

/// Náš umělý limit pro velikost disku
pub const MAX_DISK_SIZE: u64 = 4 * GIB;
