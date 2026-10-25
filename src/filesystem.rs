//! Souborový systém pseudoFAT
//!
//! [`FileSystem`] drží otevřený soubor („disk“), jeho rozložení a FAT.

mod bytes;
mod dir;
mod dir_ops;
mod disk;
mod fat;
mod file_ops;
mod path;
mod superblock;

use std::io;
use std::path::Path;

use self::dir::Dir;
use self::disk::Disk;
use self::fat::{FAT, FATEntry};
use self::superblock::Superblock;
use crate::config::CLUSTER_SIZE;
use crate::error::{FsError, cannot_create};

/// Otevřený souborový systém
pub struct FileSystem {
    /// Soubor se souborovým systémem
    disk: Disk,
    /// Rozložení disku
    sb: Superblock,
    /// FAT v paměti, změny se hned zapisují i na disk
    fat: FAT,
    /// Cluster aktuálního adresáře – jen stav běžícího programu, na disk se neukládá
    cwd: u32,
}

impl FileSystem {
    /// Naformátuje soubor `path` na prázdný FS o velikosti `size` bajtů
    /// Původní obsah souboru se zahodí, neexistující soubor se vytvoří
    pub fn format(path: &Path, size: u64) -> Result<FileSystem, FsError> {
        // Rozložení spočítáme dřív, než na soubor sáhneme: při nesmyslné
        // velikosti tak původní obsah souboru zůstane nedotčený
        let sb = Superblock::new(size, CLUSTER_SIZE).map_err(cannot_create)?;

        let mut disk = Disk::create(path, size).map_err(cannot_create)?;
        sb.save(&mut disk).map_err(cannot_create)?;

        // Nový soubor obsahuje samé nuly a 0 by znamenala Next(0), ne FREE –
        // FAT je proto potřeba výslovně vyplnit (tj. FAT::new_free)
        let mut fat = FAT::new_free(sb.fat_start, sb.cluster_count);
        fat.save_all(&mut disk).map_err(cannot_create)?;

        // Kořenový adresář: zabere cluster určený superblokem (jako každý adresář právě jeden,
        // proto hned EOF), jeho ".." ukazuje na něj samotného
        fat.set(&mut disk, sb.root_cluster, FATEntry::EOF).map_err(cannot_create)?;
        Dir::init(&mut disk, &sb, sb.root_cluster, sb.root_cluster).map_err(cannot_create)?;

        let cwd = sb.root_cluster;
        Ok(FileSystem { disk, sb, fat, cwd })
    }

    /// Otevře existující FS ze souboru `path`
    pub fn open(path: &Path) -> Result<FileSystem, FsError> {
        let mut disk = match Disk::open(path) {
            Ok(disk) => disk,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(FsError::NotFormatted),
            // Ostatní je FsError::Io
            Err(e) => return Err(e.into()),
        };

        let sb = Superblock::load(&mut disk)?;

        // Superblok je ověřený, FAT tedy leží celá uvnitř souboru
        let fat = FAT::load(&mut disk, sb.fat_start, sb.cluster_count)?;

        // Po spuštění programu je aktuálním adresářem vždy kořen
        let cwd = sb.root_cluster;
        Ok(FileSystem { disk, sb, fat, cwd })
    }

    /// Rozložení disku
    pub fn superblock(&self) -> &Superblock {
        &self.sb
    }

    /// Počet volných clusterů
    pub fn free_clusters(&self) -> u32 {
        self.fat.free_count()
    }
}
