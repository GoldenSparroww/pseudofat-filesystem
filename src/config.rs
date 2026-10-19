//! Konfigurovatelné parametry pseudoFAT.
//!
//! Hodnoty se mění přepsáním konstanty a novým překladem programu.

// ---------------------------------------------------------------------------
// Jednotky (binární, mocniny 2)
// ---------------------------------------------------------------------------

/// Kibibajt: 1 KiB = 1024 B.
pub const KIB: u64 = 1024;

/// Mebibajt: 1 MiB = 1024 KiB = 1 048 576 B.
pub const MIB: u64 = 1024 * KIB;

// ---------------------------------------------------------------------------
// Příkaz format
// ---------------------------------------------------------------------------

/// Počet bajtů, které představuje jednotka „MB“ v příkazu `format 600MB`.
///
/// - `MIB`       – binární jednotka, 1 MB = 1 048 576 B (výchozí)
/// - `1_000_000` – desítková jednotka, 1 MB = 1 000 000 B
pub const FORMAT_UNIT: u64 = MIB;
