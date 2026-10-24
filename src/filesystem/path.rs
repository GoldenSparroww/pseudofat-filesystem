//! Překlad cest uvnitř pseudoFAT
//!
//! Cesta je absolutní (začíná `/`, překlad začne v kořeni), nebo relativní (začne v aktuálním
//! adresáři). Části oddělené `/` se zpracují zleva doprava:
//!
//! - `.` – zůstane se v adresáři,
//! - `..` – přejde se do rodiče (podle položky `..`, kořen je rodičem sám sobě),
//! - jméno – přejde se do podadresáře, který musí existovat (jinak PATH NOT FOUND).
//!
//! Prázdné části (`a//b`, `a/`) se ignorují

use super::FileSystem;
use super::dir::{Dir, DirEntry, EntryKind, Name};
use super::fat::FATEntry;
use crate::error::{FsError, corrupted};

impl FileSystem {
    /// Načte adresář z clusteru `cluster`
    pub(super) fn load_dir(&mut self, cluster: u32) -> Result<Dir, FsError> {
        // Dvě výpůjčky různých polí self najednou (disk mutable, sb sdíleně) jsou v pořádku
        Dir::load(&mut self.disk, &self.sb, cluster)
    }

    /// Cluster existujícího adresáře na cestě `path`
    pub(super) fn resolve_dir(&mut self, path: &str) -> Result<u32, FsError> {
        let (cluster, last) = self.walk(path)?;

        match last {
            None => Ok(cluster),
            Some(name) => self.child_dir(cluster, name),
        }
    }

    /// Rozloží cestu nové položky na rodičovský adresář a jméno nové položky
    ///
    /// Ověří vše, co lze ověřit před vytvořením:
    /// - rodič existuje → jinak [`FsError::PathNotFound`],
    /// - jméno je platné 8.3 → jinak [`FsError::InvalidName`],
    /// - položka ještě neexistuje → jinak [`FsError::Exist`] (`.`, `..` a `/` existují vždy),
    /// - v rodiči je volný slot → jinak [`FsError::DirFull`].
    pub(super) fn resolve_new(&mut self, path: &str) -> Result<(Dir, Name), FsError> {
        let (parent, last) = self.walk(path)?;

        let Some(name) = last else {
            return Err(FsError::Exist);
        };
        let name = Name::parse(name)?;

        let dir = self.load_dir(parent)?;
        if dir.find(&name).is_some() {
            return Err(FsError::Exist);
        }
        if dir.is_full() {
            return Err(FsError::DirFull);
        }

        Ok((dir, name))
    }

    /// Absolutní cesta k adresáři `cluster`, např. `/a/b`
    ///
    /// Adresář nezná své jméno – to je uložené v rodiči. Postupuje se proto přes `..` nahoru ke
    /// kořeni a v každém rodiči se hledá položka, která ukazuje na adresář o úroveň níž
    pub(super) fn path_of(&mut self, cluster: u32) -> Result<String, FsError> {
        let mut names = Vec::new();
        let mut current = cluster;

        while current != self.sb.root_cluster {
            // V poškozeném FS může `..` vést do kruhu. Strom hlubší než počet clusterů
            // neexistuje (každý adresář má vlastní cluster)
            if names.len() >= self.sb.cluster_count as usize {
                return Err(corrupted("cycle in directory tree"));
            }

            let parent = self.parent_of(current)?;
            let parent_dir = self.load_dir(parent)?;
            let Some(entry) = parent_dir
                .children()
                .find(|entry| entry.kind == EntryKind::Dir && entry.start == FATEntry::Next(current))
            else {
                return Err(corrupted(format!(
                    "directory cluster {current} not found in its parent"
                )));
            };

            names.push(entry.name.to_string());
            current = parent;
        }

        // Jména jsou posbíraná od nejhlubšího ke kořeni
        names.reverse();
        Ok(format!("/{}", names.join("/")))
    }

    /// Projde cestu `path` až po poslední část
    ///
    /// Vrátí cluster adresáře, ve kterém překlad skončil, a poslední část cesty, pokud je to
    /// obyčejné jméno – co s ním udělat (najít, vytvořit, …), rozhodne volající:
    ///
    /// | cesta    | výsledek                     |
    /// |----------|------------------------------|
    /// | `a/b/c`  | (cluster `a/b`, `Some("c")`) |
    /// | `a/b/..` | (cluster `a`, `None`)        |
    /// | `/`      | (kořen, `None`)              |
    ///
    /// Chybějící adresář po cestě nebo soubor místo adresáře vrátí [`FsError::PathNotFound`]
    fn walk<'p>(&mut self, path: &'p str) -> Result<(u32, Option<&'p str>), FsError> {
        let mut cluster = if path.starts_with('/') {
            self.sb.root_cluster
        } else {
            self.cwd
        };

        let mut parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();

        // Poslední část se nechá volajícímu, jen když je to jméno – `.` a `..` se vyřeší hned
        let last = parts.last().copied().filter(|&part| part != "." && part != "..");
        if last.is_some() {
            parts.pop();
        }

        for part in parts {
            cluster = match part {
                "." => cluster,
                ".." => self.parent_of(cluster)?,
                name => self.child_dir(cluster, name)?,
            };
        }

        Ok((cluster, last))
    }

    /// Cluster podadresáře `name` v adresáři `cluster`
    fn child_dir(&mut self, cluster: u32, name: &str) -> Result<u32, FsError> {
        // Neplatné jméno v adresáři být nemůže – neexistuje
        let Ok(name) = Name::parse(name) else {
            return Err(FsError::PathNotFound);
        };

        let dir = self.load_dir(cluster)?;
        match dir.find(&name) {
            Some(entry) if entry.kind == EntryKind::Dir => dir_cluster(entry),
            // Neexistuje, nebo je to soubor
            _ => Err(FsError::PathNotFound),
        }
    }

    /// Cluster rodiče adresáře `cluster` (podle jeho položky `..`)
    fn parent_of(&mut self, cluster: u32) -> Result<u32, FsError> {
        let dir = self.load_dir(cluster)?;

        match dir.find(&Name::DOTDOT) {
            Some(entry) => dir_cluster(entry),
            None => Err(corrupted(format!("directory cluster {cluster} has no '..' entry"))),
        }
    }
}

/// Cluster adresáře, na který ukazuje položka `entry`
///
/// Adresář má vždy právě jeden cluster, `start` proto musí být `Next(c)`
fn dir_cluster(entry: &DirEntry) -> Result<u32, FsError> {
    match entry.start {
        FATEntry::Next(cluster) => Ok(cluster),
        other => Err(corrupted(format!("directory {} starts with {other:?}", entry.name))),
    }
}
