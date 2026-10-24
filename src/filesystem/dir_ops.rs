//! Příkazy pro práci s adresáři: mkdir, ls, cd, pwd

use super::FileSystem;
use super::dir::{Dir, DirEntry, EntryKind};
use crate::error::FsError;

impl FileSystem {
    /// mkdir: vytvoří prázdný adresář
    pub fn mkdir(&mut self, path: &str) -> Result<(), FsError> {
        // Všechny kontroly (rodič, jméno, EXIST, volný slot)
        let (mut parent, name) = self.resolve_new(path)?;

        // Pořadí zápisů: FAT a obsah nového adresáře nejdřív, položka v rodiči až nakonec.
        // Při pádu mezi nimi zůstane jen cluster bez vlastníka, žádná položka do prázdna
        // allocate vrací přesně tolik clusterů, kolik se požaduje – tedy jeden
        let cluster = self.fat.allocate(&mut self.disk, 1)?[0];
        Dir::init(&mut self.disk, &self.sb, cluster, parent.cluster())?;
        parent.insert(&mut self.disk, DirEntry::dir(name, cluster))
    }

    /// ls: řádky výpisu adresáře `path` (bez parametru aktuálního) bez `.` a `..`, v pořadí slotů
    pub fn ls(&mut self, path: Option<&str>) -> Result<Vec<String>, FsError> {
        let cluster = match path {
            Some(path) => self.resolve_dir(path)?,
            None => self.cwd,
        };

        let dir = self.load_dir(cluster)?;
        Ok(dir.children().map(ls_line).collect())
    }

    /// cd: změní aktuální adresář
    pub fn cd(&mut self, path: &str) -> Result<(), FsError> {
        // Přiřazuje se až po úspěšném překladu – při chybě aktuální adresář zůstane
        self.cwd = self.resolve_dir(path)?;
        Ok(())
    }

    /// pwd: absolutní cesta aktuálního adresáře
    pub fn pwd(&mut self) -> Result<String, FsError> {
        self.path_of(self.cwd)
    }
}

/// Řádek výpisu ls ve tvaru ze zadání (`FILE: f1`, `DIR: a2`)
fn ls_line(entry: &DirEntry) -> String {
    match entry.kind {
        EntryKind::File => format!("FILE: {}", entry.name),
        EntryKind::Dir => format!("DIR: {}", entry.name),
    }
}
