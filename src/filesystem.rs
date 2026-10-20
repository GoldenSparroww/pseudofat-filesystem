//! Souborový systém pseudoFAT
//!
//! [`FileSystem`] drží otevřený soubor („disk“), jeho rozložení a FAT.

mod disk;
mod fat;
mod superblock;

use std::io;
use std::path::Path;

use self::disk::Disk;
use self::fat::FAT;
use self::superblock::Superblock;
use crate::config::CLUSTER_SIZE;
use crate::error::FsError;

/// Otevřený souborový systém
pub struct FileSystem {
    /// Soubor se souborovým systémem
    disk: Disk,
    /// Rozložení disku
    sb: Superblock,
    /// FAT v paměti, změny se hned zapisují i na disk
    fat: FAT,
}

impl FileSystem {
    /// Naformátuje soubor `path` na prázdný FS o velikosti `size` bajtů
    /// Původní obsah souboru se zahodí, neexistující soubor se vytvoří
    pub fn format(path: &Path, size: u64) -> Result<FileSystem, FsError> {
        // Rozložení spočítáme dřív, než na soubor sáhneme: při nesmyslné
        // velikosti tak původní obsah souboru zůstane nedotčený
        let sb = Superblock::new(size, CLUSTER_SIZE).map_err(cannot_create)?;

        let mut disk = Disk::create(path, size).map_err(cannot_create)?;
        disk.write_at(0, &sb.to_bytes()).map_err(cannot_create)?;

        // Nový soubor obsahuje samé nuly a 0 by znamenala Next(0), ne FREE –
        // FAT je proto potřeba výslovně vyplnit (tj. FAT::new_free)
        let fat = FAT::new_free(sb.fat_start, sb.cluster_count);
        fat.save_all(&mut disk).map_err(cannot_create)?;

        // TODO krok 4: založit kořenový adresář

        Ok(FileSystem { disk, sb, fat })
    }

    /// Otevře existující FS ze souboru `path`
    pub fn open(path: &Path) -> Result<FileSystem, FsError> {
        let mut disk = match Disk::open(path) {
            Ok(disk) => disk,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(FsError::NotFormatted),
            // Ostatní je FsError::Io
            Err(e) => return Err(e.into()),
        };

        // Převod je díky implementaci From automatický na FsError
        let disk_size = disk.size()?;

        // Tady jen hlídáme, že jde přečíst celý superblok
        // Úplnou podmínku (superblok + jedna položka FAT + jeden cluster) ověří from_bytes
        // přes Superblock::from_bytes
        if disk_size < superblock::SUPERBLOCK_SIZE as u64 {
            return Err(FsError::NotFormatted);
        }

        let mut bytes = [0u8; superblock::SUPERBLOCK_SIZE];
        disk.read_at(0, &mut bytes)?;
        let sb = Superblock::from_bytes(&bytes, disk_size)?;

        // Superblok je ověřený, FAT tedy leží celá uvnitř souboru
        let fat = FAT::load(&mut disk, sb.fat_start, sb.cluster_count)?;

        Ok(FileSystem { disk, sb, fat })
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

/// Helper: Chyba formátu s důvodem (text chyby OS nebo výpočtu rozložení)
fn cannot_create(reason: impl ToString) -> FsError {
    FsError::CannotCreateFile(reason.to_string())
}
