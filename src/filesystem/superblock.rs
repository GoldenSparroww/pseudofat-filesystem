//! Superblok – popis rozložení FS, uložený na začátku souboru

use std::fmt;

use super::fat;
use crate::config::{CLUSTER_SIZE, FORMAT_UNIT, MAX_DISK_SIZE};
use crate::error::FsError;

/// Velikost oblasti vyhrazené pro superblok
pub const SUPERBLOCK_SIZE: usize = 64;

/// Identifikace našeho FS – prvních 8 bajtů souboru
const MAGIC: [u8; 8] = *b"PSEUDFAT";

/// Verze formátu na disku, zvýšit při každé nekompatibilní změně
const VERSION: u32 = 1;

/// Nejmenší povolená velikost clusteru
const MIN_CLUSTER_SIZE: u32 = 512;

// Kontrola konfigurace už při překladu: s příliš malým clusterem se program nepřeloží
const _: () = assert!(CLUSTER_SIZE >= MIN_CLUSTER_SIZE, "CLUSTER_SIZE must be at least 512 B");

// I největší disk s nejmenším clusterem musí jít adresovat čísly clusterů typu u32
const _: () = assert!(
    MAX_DISK_SIZE / (MIN_CLUSTER_SIZE as u64 + fat::FAT_INDEX_SIZE) <= fat::MAX_CLUSTER_COUNT as u64,
    "MAX_DISK_SIZE is too large for u32 cluster numbers"
);

// Pozice polí
const AT_MAGIC: usize = 0;
const AT_VERSION: usize = 8;
const AT_CLUSTER_SIZE: usize = 12;
const AT_CLUSTER_COUNT: usize = 16;
const AT_ROOT_CLUSTER: usize = 20;
const AT_DISK_SIZE: usize = 24;
const AT_FAT_START: usize = 32;
const AT_DATA_START: usize = 40;

/// Rozložení souborového systému na disku
#[derive(Debug, PartialEq, Eq)]
pub struct Superblock {
    /// Velikost clusteru v bajtech
    pub cluster_size: u32,
    /// Počet clusterů = počet položek FAT
    pub cluster_count: u32,
    /// Cluster kořenového adresáře
    pub root_cluster: u32,
    /// Celková velikost disku v bajtech
    pub disk_size: u64,
    /// Offset začátku FAT
    pub fat_start: u64,
    /// Offset clusteru 0
    pub data_start: u64,
}

impl Superblock {
    /// Spočítá rozložení FS pro disk o velikosti `disk_size` bajtů
    pub fn new(disk_size: u64, cluster_size: u32) -> Result<Superblock, String> {
        if cluster_size < MIN_CLUSTER_SIZE {
            return Err(format!(
                "Cluster size too small (minimum allowed: {} B).",
                MIN_CLUSTER_SIZE
            ));
        }

        if disk_size > MAX_DISK_SIZE {
            return Err(format!(
                "Disk too large (maximum allowed: {} MB).",
                MAX_DISK_SIZE / FORMAT_UNIT
            ));
        }

        let fat_start = SUPERBLOCK_SIZE as u64;
        // e.g. cluster size + FAT index = 1024 + 4 = 1028
        let bytes_per_cluster = u64::from(cluster_size) + fat::FAT_INDEX_SIZE;
        // saturating_sub: disk menší než superblok dá 0, místo aby odečítání podteklo
        // Celočíselné dělení zbytek zahodí, pokud není přesné
        let count = disk_size.saturating_sub(fat_start) / bytes_per_cluster;

        // Zde se ověřuje jestli dokáže náš FAT (u32) adresovat požadavoný počet clusterů
        let cluster_count = match u32::try_from(count) {
            // Pokud vstupní count = 0, u32::try_from(count) = 0 a tedy
            // disk_size.saturating_sub(fat_start) = 0; což indikuje vstupní disk_size < fat_start
            Ok(0) => {
                return Err(format!(
                    "Disk too small. Condition: Disk size >= Superblock + one FAT index + one cluster. ({} >= {} + {} + {})",
                    disk_size,
                    SUPERBLOCK_SIZE,
                    fat::FAT_INDEX_SIZE,
                    cluster_size
                ));
            }
            // Nejen respektovat u32::try_from ale i naše omezení
            Ok(n) if n <= fat::MAX_CLUSTER_COUNT => n,
            // Jinak clustery nedokážeme do našich podmínek uložit
            _ => return Err(format!("Disk too large. Icrease cluster size, or lower disk size.")),
        };

        Ok(Superblock {
            cluster_size,
            cluster_count,
            root_cluster: 0,
            disk_size,
            fat_start,
            data_start: fat_start + count * fat::FAT_INDEX_SIZE,
        })
    }

    /// Vrátí jen superblok, který popisuje platný FS
    pub fn from_bytes(bytes: &[u8; SUPERBLOCK_SIZE], disk_size: u64) -> Result<Superblock, FsError> {
        if get_bytes(bytes, AT_MAGIC) != MAGIC {
            return Err(FsError::NotFormatted);
        }

        let version = get_u32(bytes, AT_VERSION);
        if version != VERSION {
            return Err(corrupted(format!("unsupported version {version}")));
        }

        let sb = Superblock {
            cluster_size: get_u32(bytes, AT_CLUSTER_SIZE),
            cluster_count: get_u32(bytes, AT_CLUSTER_COUNT),
            root_cluster: get_u32(bytes, AT_ROOT_CLUSTER),
            disk_size: get_u64(bytes, AT_DISK_SIZE),
            fat_start: get_u64(bytes, AT_FAT_START),
            data_start: get_u64(bytes, AT_DATA_START),
        };

        // Uložená velikost musí sedět se skutečnou délkou souboru...
        if sb.disk_size != disk_size {
            return Err(corrupted(format!(
                "superblock says {} B, file has {disk_size} B",
                sb.disk_size
            )));
        }

        // ...a zbytek musí přesně odpovídat rozložení, které by pro stejný disk
        // a cluster spočítal format
        let expected = Superblock::new(sb.disk_size, sb.cluster_size).map_err(corrupted)?;
        if sb != expected {
            return Err(corrupted("inconsistent layout"));
        }

        Ok(sb)
    }

    /// Převede superblok na bajty pro zápis na začátek disku
    pub fn to_bytes(&self) -> [u8; SUPERBLOCK_SIZE] {
        let mut bytes = [0u8; SUPERBLOCK_SIZE];

        put_bytes(&mut bytes, AT_MAGIC, &MAGIC);
        put_u32(&mut bytes, AT_VERSION, VERSION);
        put_u32(&mut bytes, AT_CLUSTER_SIZE, self.cluster_size);
        put_u32(&mut bytes, AT_CLUSTER_COUNT, self.cluster_count);
        put_u32(&mut bytes, AT_ROOT_CLUSTER, self.root_cluster);
        put_u64(&mut bytes, AT_DISK_SIZE, self.disk_size);
        put_u64(&mut bytes, AT_FAT_START, self.fat_start);
        put_u64(&mut bytes, AT_DATA_START, self.data_start);

        bytes
    }
}

impl fmt::Display for Superblock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} B, {} clusters x {} B, FAT at {}, data at {}",
            self.disk_size, self.cluster_count, self.cluster_size, self.fat_start, self.data_start
        )
    }
}

/// Chyba poškozeného superbloku s popisem problému
fn corrupted(detail: impl ToString) -> FsError {
    FsError::Corrupted(detail.to_string())
}

/// Zapíše bajty `value` na pozici `at`
fn put_bytes(bytes: &mut [u8], at: usize, value: &[u8]) {
    bytes[at..at + value.len()].copy_from_slice(value);
}

/// Přečte `N` bajtů z pozice `at` jako pole `[u8; N]`, `N` se odvodí z místa použití
fn get_bytes<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
    let mut buf = [0u8; N];
    buf.copy_from_slice(&bytes[at..at + N]);
    buf
}

/// Zapíše `value` jako 4 bajty little-endian na pozici `at`
fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    put_bytes(bytes, at, &value.to_le_bytes());
}

/// Zapíše `value` jako 8 bajtů little-endian na pozici `at`
fn put_u64(bytes: &mut [u8], at: usize, value: u64) {
    put_bytes(bytes, at, &value.to_le_bytes());
}

/// Přečte 4 bajty little-endian z pozice `at`
fn get_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(get_bytes(bytes, at))
}

/// Přečte 8 bajtů little-endian z pozice `at`
fn get_u64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(get_bytes(bytes, at))
}
