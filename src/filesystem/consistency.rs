//! Consistency helpers
//! 
//! Used by `bug` and `check` commands

use std::fmt;

use crate::filesystem::{
    FileSystem, FsError,
    dir::{Dir, DirEntry, DirEntryKind},
    fat::FATPtrEntry,
};

// ===== BugKind ======

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BugKind {
    /// Size in the directory entry does not match the chain length
    SizeDiffer,
    /// Last cluster of the chain points back to the first one
    CycleFAT,
    /// Last cluster of the chain points outside of the cluster range
    InvalidClusterNum,
}

impl BugKind {
    pub fn parse(text: &str) -> Option<BugKind> {
        match text {
            "size" => Some(BugKind::SizeDiffer),
            "cycle" => Some(BugKind::CycleFAT),
            "invalid" => Some(BugKind::InvalidClusterNum),
            _ => None,
        }
    }
}

impl fmt::Display for BugKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BugKind::SizeDiffer => write!(f, "size"),
            BugKind::CycleFAT => write!(f, "cycle"),
            BugKind::InvalidClusterNum => write!(f, "invalid"),
        }
    }
}

// ===== check ======

impl FileSystem {
    /// Walks the whole directory tree and returns one line per problem found (empty = OK).
    /// Checks every file for an invalid cluster number, a cycle in the chain and a size/chain mismatch
    pub fn check(&self) -> Result<Vec<String>, FsError> {
        let mut problems = Vec::new();
        // Protects the walk itself: a corrupted entry could point back to an already visited directory
        let mut visited = vec![false; self.superblock.cluster_count as usize];

        self.check_dir(self.superblock.root_cluster, "", &mut visited, &mut problems)?;
        Ok(problems)
    }

    /// Recursive walk through directory tree
    fn check_dir(&self, cluster: u32, path: &str, visited: &mut [bool], problems: &mut Vec<String>) -> Result<(), FsError> {
        let shown = if path.is_empty() { "/" } else { path };

        match visited.get_mut(cluster as usize) {
            Some(true) => {
                problems.push(format!("{shown}: directory cluster {cluster} visited twice (cycle in directory tree)"));
                return Ok(());
            }
            Some(seen) => *seen = true,
            None => {
                problems.push(format!("{shown}: invalid directory cluster {cluster}"));
                return Ok(());
            }
        }

        let dir = match self.get_dir(cluster) {
            Ok(dir) => dir,
            Err(FsError::Corrupted(msg)) => {
                problems.push(format!("{shown}: {msg}"));
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        for entry in dir.get_slots() {
            let entry_path = format!("{path}/{}", entry.name);

            match entry.kind {
                DirEntryKind::File { size } => self.check_file(&entry_path, entry, size, problems),
                DirEntryKind::Dir => match self.get_dir_cluster(entry) {
                    Some(child) => self.check_dir(child, &entry_path, visited, problems)?,
                    None => problems.push(format!("{entry_path}: directory has no cluster")),
                },
            }
        }

        Ok(())
    }

    fn check_file(&self, path: &str, entry: &DirEntry, size: u32, problems: &mut Vec<String>) {
        // get_chain already detects an invalid cluster number, a cycle and FREE/BAD inside the chain
        let clusters = match self.fat.get_chain(entry.start) {
            Ok(clusters) => clusters,
            Err(e) => {
                problems.push(format!("{path}: {e}"));
                return;
            }
        };

        let needed = (size as usize).div_ceil(self.superblock.cluster_size as usize);
        if clusters.len() != needed {
            problems.push(format!(
                "{path}: size {size} B needs {needed} clusters, chain has {}",
                clusters.len()
            ));
        }
    }
}

// ===== bug ======

impl FileSystem {
    /// Kinds of corruption that can be applied to the file `entry`.
    /// `size` always works; `cycle` and `invalid` need at least one cluster and an intact chain
    pub fn get_bug_kinds(&self, entry: &DirEntry) -> Vec<BugKind> {
        match self.fat.get_chain(entry.start) {
            Ok(clusters) if !clusters.is_empty() => vec![BugKind::SizeDiffer, BugKind::CycleFAT, BugKind::InvalidClusterNum],
            _ => vec![BugKind::SizeDiffer], // when start is eof AND get_chain retunered error (something is already broken)
        }
    }

    /// Corrupts the file `entry` in `parent`. The caller checks that `kind` is in `get_bug_kinds`
    pub fn bug(&mut self, parent: &mut Dir, entry: &DirEntry, kind: BugKind) -> Result<(), FsError> {
        let DirEntryKind::File { size } = entry.kind else {
            return Err(FsError::FileNotFound);
        };

        match kind {
            BugKind::SizeDiffer => {
                // One cluster more (or less, if it would overflow) always changes the needed cluster count
                let cluster_size = self.superblock.cluster_size;
                let new_size = size.checked_add(cluster_size).unwrap_or_else(|| size - cluster_size);
                let corrupted = DirEntry::file(entry.name, new_size, entry.start);
                parent.replace(&mut self.disk, &entry.name, corrupted)
            }
            BugKind::CycleFAT | BugKind::InvalidClusterNum => {
                let clusters = self.fat.get_chain(entry.start)?;
                let (Some(&first), Some(&last)) = (clusters.first(), clusters.last()) else {
                    return Err(FsError::Corrupted(format!("file {} has no clusters", entry.name)));
                };

                let next = match kind {
                    BugKind::CycleFAT => first,
                    // cluster_count is the first number outside the range
                    _ /* BugKind::InvalidClusterNum */ => self.superblock.cluster_count,
                };
                self.fat.set(&mut self.disk, last, FATPtrEntry::Next(next))
            }
        }
    }
}
