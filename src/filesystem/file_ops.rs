//! Příkazy pro práci se soubory: data souboru v clusterech (cat, incp, outcp) a info
//!
//! Soubor na hostiteli tady neznáme – pseudoFAT dostává a vrací jen bajty, se soubory na
//! hostiteli pracuje shell (přes `host.rs`)

use std::ops::Range;

use super::FileSystem;
use super::dir::{DIR_ENTRY_SIZE, DirEntry};
use super::fat::FATEntry;
use super::path::Node;
use crate::error::{FsError, corrupted};

impl FileSystem {
    /// TODO oddelit
    /// Obsah souboru na cestě `path` (cat, outcp)
    pub fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        let (_, entry) = self.resolve_file(path)?;
        self.read_data(&entry)
    }

    /// TODO oddelit
    /// Vytvoří soubor na cestě `path` s obsahem `data` (incp)
    pub fn create_file(&mut self, path: &str, data: &[u8]) -> Result<(), FsError> {
        let (mut parent, name) = self.resolve_new(path)?;

        // Velikost se ukládá jako u32. Větší soubor by se na disk (nejvýš MAX_DISK_SIZE) nevešel
        let size = u32::try_from(data.len()).map_err(|_| FsError::DiskFull)?;

        // Pořadí zápisů: FAT, data, až nakonec položka v adresáři. Při pádu uprostřed zůstanou
        // jen clustery bez vlastníka, nikdy položka ukazující na nezapsaná data
        let count = self.clusters_needed(size);
        let clusters = self.fat.allocate(&mut self.disk, count)?;
        for (offset, range) in self.extents(&clusters, data.len()) {
            self.disk.write_at(offset, &data[range])?;
        }

        // Prázdný soubor nemá žádný cluster
        let start = match clusters.first() {
            Some(&first) => FATEntry::Next(first),
            None => FATEntry::EOF,
        };
        parent.insert(&mut self.disk, DirEntry::file(name, size, start))
    }

    /// info
    pub fn info(&mut self, path: &str) -> Result<Vec<String>, FsError> {
        match self.resolve_node(path)? {
            Node::File { parent, entry } => self.file_info(parent.cluster(), &entry),
            Node::Dir(cluster) => self.dir_info(cluster),
        }
    }

    /// info souboru `entry`, který leží v adresáři `parent`
    fn file_info(&mut self, parent: u32, entry: &DirEntry) -> Result<Vec<String>, FsError> {
        // Na rozdíl od čtení se tu délka řetězce s velikostí nekontroluje – info má nesoulad
        // ukázat (třeba po příkazu bug), ne odmítnout
        let clusters = self.fat.chain(entry.start)?;
        let needed = self.clusters_needed(entry.size);

        let list = if clusters.is_empty() {
            "(no clusters)".to_string()
        } else {
            clusters.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
        };
        let mismatch = if clusters.len() == needed { "" } else { " – MISMATCH" };
        // Cesta adresáře je bez lomítka na konci, kromě kořene ("/")
        let path = format!("{}/{}", self.path_of(parent)?.trim_end_matches('/'), entry.name);

        let mut lines = vec![
            format!("{} – file – velikost: {}B - {list}", entry.name, entry.size),
            format!("  path:      {path} (entry in directory cluster {parent})"),
            format!(
                "  clusters:  {} x {} B ({} B needs {needed}){mismatch}",
                clusters.len(),
                self.sb.cluster_size,
                entry.size
            ),
        ];
        if let Some(&first) = clusters.first() {
            lines.push(format!("  fragments: {}", fragments(&clusters)));
            lines.push(format!("  data at:   offset {:#x}", self.sb.cluster_offset(first)));
        }
        Ok(lines)
    }

    /// info adresáře v clusteru `cluster`
    fn dir_info(&mut self, cluster: u32) -> Result<Vec<String>, FsError> {
        // load_dir ověří i rozsah čísla clusteru, get níže tedy vždy něco vrátí
        let dir = self.load_dir(cluster)?;
        let path = self.path_of(cluster)?;
        let parent = self.parent_of(cluster)?;

        // Jméno je poslední část cesty, kořen žádné jméno nemá
        let name = match path.rsplit('/').next() {
            Some("") | None => "/",
            Some(name) => name,
        };
        let capacity = self.sb.cluster_size as usize / DIR_ENTRY_SIZE - 2;

        let mut lines = vec![
            format!("{name} – dir – cluster - {cluster}"),
            format!("  path:      {path} (parent directory cluster {parent})"),
            format!(
                "  entries:   {} of {capacity} slots used (without . and ..)",
                dir.children().count()
            ),
            format!("  data at:   offset {:#x}", self.sb.cluster_offset(cluster)),
        ];
        if let Some(entry) = self.fat.get(cluster) {
            // Adresář má právě jeden cluster, správně je tu EOF
            lines.push(format!("  FAT:       [{cluster}] = {entry:?}"));
        }
        Ok(lines)
    }

    /// Obsah souboru podle jeho položky
    fn read_data(&mut self, entry: &DirEntry) -> Result<Vec<u8>, FsError> {
        let clusters = self.fat.chain(entry.start)?;

        // Kratší řetězec by nestačil na všechna data, delší znamená poškozený FS
        let needed = self.clusters_needed(entry.size);
        if clusters.len() != needed {
            return Err(corrupted(format!(
                "file {}: size {} B needs {needed} clusters, chain has {}",
                entry.name,
                entry.size,
                clusters.len()
            )));
        }

        let mut data = vec![0u8; entry.size as usize];
        for (offset, range) in self.extents(&clusters, data.len()) {
            self.disk.read_at(offset, &mut data[range])?;
        }
        Ok(data)
    }

    /// Počet clusterů pro `size` bajtů (zaokrouhleno nahoru, 0 B = žádný cluster)
    fn clusters_needed(&self, size: u32) -> usize {
        size.div_ceil(self.sb.cluster_size) as usize
    }

    /// Rozdělí data souboru na souvislé úseky na disku: (offset v souboru se FS, rozsah bajtů dat)
    ///
    /// Po sobě jdoucí clustery (např. 4, 5, 6) leží v souboru se FS hned za sebou, takže se
    /// přečtou nebo zapíšou jedním voláním místo po jednom clusteru. Poslední cluster bývá
    /// zaplněný jen zčásti – úsek končí na `size`
    fn extents(&self, clusters: &[u32], size: usize) -> Vec<(u64, Range<usize>)> {
        let cluster_size = self.sb.cluster_size as usize;
        let mut extents = Vec::new();
        let mut start = 0;

        // chunk_by rozdělí výřez na skupiny, ve kterých podmínka platí pro každé dva sousedy
        for run in clusters.chunk_by(|&a, &b| b == a + 1) {
            let end = (start + run.len() * cluster_size).min(size);
            extents.push((self.sb.cluster_offset(run[0]), start..end));
            start = end;
        }

        extents
    }
}

/// Souvislé úseky clusterů pro výpis: `[2, 3, 4, 7, 10]` → `"3 (2-4, 7, 10)"`
fn fragments(clusters: &[u32]) -> String {
    let runs: Vec<String> = clusters
        .chunk_by(|&a, &b| b == a + 1)
        .map(|run| match run {
            [first, .., last] => format!("{first}-{last}"),
            [only] => only.to_string(),
            // chunk_by prázdnou skupinu nikdy nevrátí
            [] => String::new(),
        })
        .collect();

    format!("{} ({})", runs.len(), runs.join(", "))
}
