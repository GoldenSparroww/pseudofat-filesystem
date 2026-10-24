//! FAT – tabulka přidělení clusterů

use std::io;

use super::disk::Disk;
use crate::error::{FsError, corrupted};

/// Velikost jedné položky FAT na disku v bajtech (`u32`)
pub const FAT_INDEX_SIZE: u64 = 4;

/// Tři nejvyšší hodnoty `u32` jsou vyhrazené pro stavy FREE, EOF a BAD
pub const MAX_CLUSTER_COUNT: u32 = u32::MAX - 2;

// Vyhrazené hodnoty položky na disku
const RAW_FREE: u32 = u32::MAX;
const RAW_EOF: u32 = u32::MAX - 1;
const RAW_BAD: u32 = u32::MAX - 2;

/// Jedna položka FAT
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FATEntry {
    FREE,
    EOF,
    BAD,
    Next(u32),
}

impl FATEntry {
    /// Převede hodnotu z disku na význam
    pub fn from_raw(raw: u32) -> FATEntry {
        match raw {
            RAW_FREE => FATEntry::FREE,
            RAW_EOF => FATEntry::EOF,
            RAW_BAD => FATEntry::BAD,
            next => FATEntry::Next(next),
        }
    }

    /// Převede význam na hodnotu pro disk
    pub fn to_raw(self) -> u32 {
        match self {
            FATEntry::FREE => RAW_FREE,
            FATEntry::EOF => RAW_EOF,
            FATEntry::BAD => RAW_BAD,
            FATEntry::Next(next) => next,
        }
    }
}

/// FAT držená v paměti
pub struct FAT {
    /// Offset začátku FAT v souboru (ze superbloku)
    start: u64,
    /// Položky ve tvaru, v jakém jsou na disku – index = číslo clusteru
    entries: Vec<u32>,
}

impl FAT {
    /// Nová FAT, ve které jsou všechny clustery volné
    pub fn new_free(start: u64, cluster_count: u32) -> FAT {
        FAT {
            start,
            entries: vec![RAW_FREE; cluster_count as usize],
        }
    }

    /// Načte celou FAT z disku
    pub fn load(disk: &mut Disk, start: u64, cluster_count: u32) -> io::Result<FAT> {
        let mut bytes = vec![0u8; (cluster_count as usize) * (FAT_INDEX_SIZE as usize)];
        disk.read_at(start, &mut bytes)?;

        // as_chunks::<4>() rozdělí bajty na pole [u8; 4] (každé je jedna položka) a zbytek,
        // který nikdy nevznikne, ale funkce to stejně vrací – délka je násobkem 4 (a ignorujeme)
        let (chunks, _rest) = bytes.as_chunks::<{ FAT_INDEX_SIZE as usize }>();
        let entries = chunks.iter().map(|&chunk| u32::from_le_bytes(chunk)).collect();

        Ok(FAT { start, entries })
    }

    /// Zapíše celou FAT na disk
    pub fn save_all(&self, disk: &mut Disk) -> io::Result<()> {
        match self.entries.len() {
            0 => Ok(()),
            len => self.save_range(disk, 0, len - 1),
        }
    }

    /// Počet clusterů (= počet položek)
    pub fn cluster_count(&self) -> u32 {
        self.entries.len() as u32
    }

    /// Počet volných clusterů
    pub fn free_count(&self) -> u32 {
        self.entries.iter().filter(|&&raw| raw == RAW_FREE).count() as u32
    }

    /// Položka pro cluster `cluster`, `None` pro číslo mimo rozsah
    pub fn get(&self, cluster: u32) -> Option<FATEntry> {
        // neindexuje napřímo entries[i], protože mohu načíst např. 300 000, ale mít jenom 100 000 clusterů
        self.entries.get(cluster as usize).map(|&raw| FATEntry::from_raw(raw))
    }

    /// Nastaví položku pro cluster `cluster` v RAM i na disku
    pub fn set(&mut self, disk: &mut Disk, cluster: u32, entry: FATEntry) -> io::Result<()> {
        let index = cluster as usize;
        self.entries[index] = entry.to_raw();
        self.save_range(disk, index, index)
    }

    /// Projde řetězec od `first` a vrátí čísla jeho clusterů v pořadí
    /// <br> **POZOR**: musím předat jakoby "uměle" odkaz na `first`, tedy `FATEntry::Next(first)`, jinak by mi
    /// vypadnul první cluster.
    pub fn chain(&self, first: FATEntry) -> Result<Vec<u32>, FsError> {
        let mut clusters = Vec::new();
        let mut current = first;

        loop {
            let cluster = match current {
                FATEntry::EOF => return Ok(clusters),
                FATEntry::Next(cluster) => cluster,
                FATEntry::FREE => return Err(corrupted("chain contains a FREE entry")),
                FATEntry::BAD => return Err(corrupted("chain contains a BAD entry")),
            };

            if clusters.len() >= self.entries.len() {
                return Err(corrupted("cycle in chain"));
            }

            let Some(next) = self.get(cluster) else {
                return Err(corrupted(format!("invalid cluster number {cluster}")));
            };

            clusters.push(cluster);
            current = next;
        }
    }

    /// Najde `count` volných clusterů a spojí je do řetězce s EOF. Vrátí čísla clusterů v pořadí řetězce
    pub fn allocate(&mut self, disk: &mut Disk, count: usize) -> Result<Vec<u32>, FsError> {
        // Nejdřív jen hledáme – FAT se mění, až když víme, že místa je dost
        let clusters: Vec<u32> = (0..self.cluster_count())
            .filter(|&cluster| self.entries[cluster as usize] == RAW_FREE)
            .take(count)
            .collect();

        if clusters.len() < count {
            return Err(FsError::DiskFull);
        }

        // Každý cluster ukazuje na následující, poslední je EOF
        for (i, &cluster) in clusters.iter().enumerate() {
            let entry = match clusters.get(i + 1) {
                Some(&next) => FATEntry::Next(next),
                None => FATEntry::EOF,
            };
            self.entries[cluster as usize] = entry.to_raw();
        }

        self.save_clusters(disk, &clusters)?;
        Ok(clusters)
    }

    /// Uvolní celý řetězec začínající `first` (všechny jeho clustery nastaví na FREE)
    /// Poškozený řetězec se neuvolní vůbec – vrátí chybu dřív, než se cokoliv změní
    pub fn free_chain(&mut self, disk: &mut Disk, first: FATEntry) -> Result<(), FsError> {
        let clusters = self.chain(first)?;

        for &cluster in &clusters {
            self.entries[cluster as usize] = RAW_FREE;
        }

        self.save_clusters(disk, &clusters)?;
        Ok(())
    }

    /// Zapíše na disk položky všech clusterů ze seznamu (mírna optimalizační abstrakce nad self.save_range)
    fn save_clusters(&self, disk: &mut Disk, clusters: &[u32]) -> io::Result<()> {
        // Místo zápisu po jedné položce se zapíše souvislý úsek od nejnižšího po nejvyšší
        // cluster jedním voláním – položky mezi nimi se přepíšou stejnou hodnotou, jakou už
        // mají. U velkých souborů je to řádově rychlejší než tisíce zápisů po 4 B
        let (Some(&first), Some(&last)) = (clusters.iter().min(), clusters.iter().max()) else {
            return Ok(()); // prázdný seznam, není co zapisovat
        };

        // Range zde pokrývá soubory, které dostanou zapsanou stejnou hodnotu (protože jsou v range)
        self.save_range(disk, first as usize, last as usize)
    }

    /// Zapíše na disk položky `first..=last`. Pokud sleže, v RAM stav je, na disku ne; odhalí jen check.
    /// Abstrahuje offset FAT
    fn save_range(&self, disk: &mut Disk, first: usize, last: usize) -> io::Result<()> {
        let bytes: Vec<u8> = self.entries[first..=last].iter().flat_map(|raw| raw.to_le_bytes()).collect();

        disk.write_at(self.start + (first as u64) * FAT_INDEX_SIZE, &bytes)
    }
}
