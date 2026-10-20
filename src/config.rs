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

// ---------------------------------------------------------------------------
// Souborový systém
// ---------------------------------------------------------------------------

/// Velikost clusteru v bajtech pro nově formátovaný FS (běžně 512, 1024, 2048, 4096)
///
/// Hodnota se při formátu uloží do superbloku, takže dříve vytvořený FS s jinou
/// velikostí clusteru půjde otevřít i po změně této konstanty
pub const CLUSTER_SIZE: u32 = 1024;
