//! Adresáře

use std::fmt;
use std::io;

use super::bytes::{get_bytes, get_u32, put_bytes, put_u32};
use super::disk::Disk;
use super::fat::FATEntry;
use super::superblock::Superblock;
use crate::config::MAX_DISK_SIZE;
use crate::error::FsError;
use crate::error::corrupted;

/// Velikost jedné položky adresáře v bajtech
pub const DIR_ENTRY_SIZE: usize = 32;

/// Velikost uloženého jména: 8 B jméno + 3 B přípona + ukončovací `\0`
const NAME_SIZE: usize = 12;
/// Nejdelší jméno
const BASE_LEN: usize = 8;
/// Nejdelší přípona
const EXT_LEN: usize = 3;

// Pozice polí položky (viz tabulka nahoře)
const AT_NAME: usize = 0;
const AT_KIND: usize = 12;
const AT_SIZE: usize = 16;
const AT_START: usize = 20;

// Hodnoty pole kind
const KIND_FREE: u8 = 0;
const KIND_FILE: u8 = 1;
const KIND_DIR: u8 = 2;

// TODO
const _: () = assert!(MAX_DISK_SIZE <= 1 << 32, "file sizes are stored as u32");

// Jméno 8.3 bez tečky (`a.txt` je `a\0\0\0\0\0\0\0txt\0`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Name([u8; NAME_SIZE]);

impl Name {
    pub const DOT: Name = Name(*b".\0\0\0\0\0\0\0\0\0\0\0");
    pub const DOTDOT: Name = Name(*b"..\0\0\0\0\0\0\0\0\0\0");

    /// Převede jméno zadané uživatelem (např. `report.txt`) na uložený tvar
    pub fn parse(text: &str) -> Result<Name, FsError> {
        let invalid = |reason: &str| FsError::InvalidName(format!("{text}: {reason}"));

        // Jen ASCII: znak je pak právě jeden bajt a délka v bajtech = počet znaků
        if let Some(c) = text.chars().find(|&c| !c.is_ascii_graphic() || c == '/') {
            return Err(invalid(&format!("invalid character {c:?}")));
        }

        let (base, ext) = match text.split_once('.') {
            None => (text, ""),
            Some((_, "")) => return Err(invalid("empty extension")),
            Some(parts) => parts,
        };

        if base.is_empty() {
            return Err(invalid("empty name"));
        }
        if ext.contains('.') {
            return Err(invalid("more than one dot"));
        }
        if base.len() > BASE_LEN {
            return Err(invalid("name longer than 8 characters"));
        }
        if ext.len() > EXT_LEN {
            return Err(invalid("extension longer than 3 characters"));
        }

        let mut bytes = [0u8; NAME_SIZE];
        put_bytes(&mut bytes, 0, base.as_bytes());
        put_bytes(&mut bytes, BASE_LEN, ext.as_bytes());
        Ok(Name(bytes))
    }
}

impl fmt::Display for Name {
    /// Složí jméno zpět do tvaru `jmeno.ext` 
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base = until_nul(&self.0[..BASE_LEN]);
        let ext = until_nul(&self.0[BASE_LEN..BASE_LEN + EXT_LEN]);

        // lossy: jméno z poškozeného disku nemusí být platné ASCII/UTF-8
        write!(f, "{}", String::from_utf8_lossy(base))?;
        if !ext.is_empty() {
            write!(f, ".{}", String::from_utf8_lossy(ext))?;
        }
        Ok(())
    }
}

/// Část výřezu před prvním nulovým bajtem (celý výřez, pokud žádný není)
fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

/// Druh položky
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
}

/// Jedna položka adresáře
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: Name,
    pub kind: EntryKind,
    /// Velikost v bajtech, adresář má 0
    pub size: u32,
    /// Začátek řetězce ve FAT, `EOF` = žádný cluster.
    pub start: FATEntry,
}

impl DirEntry {
    /// Položka adresáře, který leží v clusteru `cluster`
    pub fn dir(name: Name, cluster: u32) -> DirEntry {
        DirEntry {
            name,
            kind: EntryKind::Dir,
            size: 0,
            start: FATEntry::Next(cluster),
        }
    }

    /// Převede položku na 32 bajtů slotu
    fn to_bytes(&self) -> [u8; DIR_ENTRY_SIZE] {
        let mut bytes = [0u8; DIR_ENTRY_SIZE];
        let kind = match self.kind {
            EntryKind::File => KIND_FILE,
            EntryKind::Dir => KIND_DIR,
        };

        put_bytes(&mut bytes, AT_NAME, &self.name.0);
        put_bytes(&mut bytes, AT_KIND, &[kind]);
        put_u32(&mut bytes, AT_SIZE, self.size);
        put_u32(&mut bytes, AT_START, self.start.to_raw());
        bytes
    }

    /// Převede 32 bajtů slotu na položku, `None` = volný slot
    fn from_bytes(bytes: &[u8; DIR_ENTRY_SIZE]) -> Result<Option<DirEntry>, u8> {
        let kind = match bytes[AT_KIND] {
            KIND_FREE => return Ok(None),
            KIND_FILE => EntryKind::File,
            KIND_DIR => EntryKind::Dir,
            unknown => return Err(unknown),
        };

        Ok(Some(DirEntry {
            name: Name(get_bytes(bytes, AT_NAME)),
            kind,
            size: get_u32(bytes, AT_SIZE),
            start: FATEntry::from_raw(get_u32(bytes, AT_START)),
        }))
    }
}

/// Obsah jednoho adresáře načtený do paměti
pub struct Dir {
    /// Cluster, ve kterém adresář leží
    cluster: u32,
    /// Offset clusteru v souboru
    offset: u64,
    /// Sloty v pořadí na disku, `None` = volný
    slots: Vec<Option<DirEntry>>,
}

impl Dir {
    /// Zapíše do clusteru `cluster` nový prázdný adresář s položkami `.` a `..`
    pub fn init(disk: &mut Disk, sb: &Superblock, cluster: u32, parent: u32) -> io::Result<()> {
        let mut bytes = vec![0u8; sb.cluster_size as usize];
        put_bytes(&mut bytes, 0, &DirEntry::dir(Name::DOT, cluster).to_bytes());
        put_bytes(
            &mut bytes,
            DIR_ENTRY_SIZE,
            &DirEntry::dir(Name::DOTDOT, parent).to_bytes(),
        );

        disk.write_at(sb.cluster_offset(cluster), &bytes)
    }

    /// Načte adresář z clusteru
    pub fn load(disk: &mut Disk, sb: &Superblock, cluster: u32) -> Result<Dir, FsError> {
        // Číslo clusteru může pocházet z poškozené položky, proto se nejdřív ověří rozsah
        if cluster >= sb.cluster_count {
            return Err(corrupted(format!("directory cluster {cluster} out of range")));
        }

        let offset = sb.cluster_offset(cluster);
        let mut bytes = vec![0u8; sb.cluster_size as usize];
        disk.read_at(offset, &mut bytes)?;

        // as_chunks rozdělí cluster na sloty [u8; 32]
        let (chunks, _rest) = bytes.as_chunks::<DIR_ENTRY_SIZE>();
        let mut slots = Vec::new();
        for (index, chunk) in chunks.iter().enumerate() {
            let slot = DirEntry::from_bytes(chunk).map_err(|kind| {
                corrupted(format!(
                    "directory cluster {cluster}, slot {index}: unknown kind {kind}"
                ))
            })?;
            slots.push(slot);
        }

        Ok(Dir { cluster, offset, slots })
    }

    /// Cluster, ve kterém adresář leží
    pub fn cluster(&self) -> u32 {
        self.cluster
    }

    /// Obsazené položky v pořadí slotů (včetně `.` a `..`)
    pub fn entries(&self) -> impl Iterator<Item = &DirEntry> {
        self.slots.iter().flatten()
    }

    /// Obsah adresáře: obsazené položky bez `.` a `..`
    pub fn children(&self) -> impl Iterator<Item = &DirEntry> {
        self.entries()
            .filter(|entry| entry.name != Name::DOT && entry.name != Name::DOTDOT)
    }

    /// Žádný slot není volný
    pub fn is_full(&self) -> bool {
        self.slots.iter().all(Option::is_some)
    }

    /// Položka se jménem `name`
    pub fn find(&self, name: &Name) -> Option<&DirEntry> {
        self.entries().find(|entry| entry.name == *name)
    }

    /// Přidá položku do prvního volného slotu
    pub fn insert(&mut self, disk: &mut Disk, entry: DirEntry) -> Result<(), FsError> {
        if self.find(&entry.name).is_some() {
            return Err(FsError::Exist);
        }

        let Some(index) = self.slots.iter().position(Option::is_none) else {
            return Err(FsError::DirFull);
        };

        self.slots[index] = Some(entry);
        self.save_slot(disk, index)?;
        Ok(())
    }

    /// Odebere položku se jménem `name` a vrátí ji, `None` = taková položka není
    pub fn remove(&mut self, disk: &mut Disk, name: &Name) -> io::Result<Option<DirEntry>> {
        let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|e| e.name == *name))
        else {
            return Ok(None);
        };

        let removed = self.slots[index].take();
        self.save_slot(disk, index)?;
        Ok(removed)
    }

    /// Zapíše na disk slot `index` (volný slot jako samé nuly)
    fn save_slot(&self, disk: &mut Disk, index: usize) -> io::Result<()> {
        let bytes = match &self.slots[index] {
            Some(entry) => entry.to_bytes(),
            None => [0u8; DIR_ENTRY_SIZE],
        };

        disk.write_at(self.offset + (index * DIR_ENTRY_SIZE) as u64, &bytes)
    }
}

