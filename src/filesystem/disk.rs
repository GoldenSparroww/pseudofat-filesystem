//! Nejnižší vrstva: čtení a zápis bajtů na daném offsetu souboru se FS
//!
//! O souborech, adresářích ani clusterech tady nic nevíme, jen o bajtech

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub struct Disk {
    file: File,
}

impl Disk {
    /// Vytvoří disk o velikosti `size` bajtů vyplněný nulami
    /// Neexistující soubor se vytvoří, existující se nejdřív zkrátí na 0 B, takže
    /// z původního obsahu nic nezůstane
    pub fn create(path: &Path, size: u64) -> io::Result<Disk> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        // Prodloužení doplní nuly. Na Linuxu vznikne řídký soubor.
        file.set_len(size)?;

        Ok(Disk { file })
    }

    /// Otevře existující disk pro čtení i zápis
    pub fn open(path: &Path) -> io::Result<Disk> {
        // Neexistující soubor se nevytváří – vrátí chybu druhu `NotFound`
        let file = OpenOptions::new().read(true).write(true).open(path)?;

        Ok(Disk { file })
    }

    /// Skutečná velikost souboru v bajtech
    pub fn size(&self) -> io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }

    /// Přečte přesně `buf.len()` bajtů od pozice `offset`
    pub fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(buf)
    }

    /// Zapíše všechny bajty `data` na pozici `offset`
    pub fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(data)
    }
}
