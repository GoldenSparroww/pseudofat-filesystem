use crate::filesystem::{
    FileSystem, FsError,
    dir::{Dir, DirEntry, DirEntryKind, Name},
    fat::FATPtrEntry,
};

// TODO, reorganize functions into less specific types
impl FileSystem {
    pub fn get_dir_cluster_from_path(&self, path: &str) -> Result<u32, FsError> {
        let (cluster, last_part) = self.walk(path)?;

        match last_part {
            None => Ok(cluster),
            Some(name) => self.get_child_dir_cluster(cluster, name),
        }
    }

    pub fn get_file_entry_from_path(&self, path: &str) -> Result<DirEntry, FsError> {
        let not_found = |_| FsError::FileNotFound;

        let (parent, Some(name)) = self.walk(path).map_err(not_found)? else {
            return Err(FsError::FileNotFound); // `/`, `.`, `..` jsou adresáře
        };
        let name = Name::from_str(name).map_err(not_found)?;

        match self.get_dir(parent)?.find(&name) {
            Some(entry @ DirEntry { kind: DirEntryKind::File { .. }, .. }) => Ok(*entry),
            _ => Err(FsError::FileNotFound),
        }
    }

    pub fn get_parent_dir_cluster(&self, curr_cluster: u32) -> Result<u32, FsError> {
        let dir = self.get_dir(curr_cluster)?;

        match dir.find(&Name::DOTDOT) {
            Some(DirEntry { kind: DirEntryKind::Dir, start: FATPtrEntry::Next(cluster), .. }) => Ok(*cluster),
            _ => Err(FsError::Corrupted(format!("directory cluster {curr_cluster} has no '..' entry"))),
        }
    }

    pub fn get_child_dir_cluster(&self, parent_dir_cluster: u32, name: &str) -> Result<u32, FsError> {
        let name = Name::from_str(name)?;

        let parent_dir = self.get_dir(parent_dir_cluster)?;

        match parent_dir.find(&name) {
            Some(DirEntry { kind: DirEntryKind::Dir, start: FATPtrEntry::Next(cluster), .. }) => Ok(*cluster),
            _ => Err(FsError::PathNotFound),
        }
    }

    pub fn get_path_from_cluster(&self, cluster: u32) -> Result<String, FsError> {
        let mut names = Vec::new();
        let mut curr_cluster = cluster;

        // A directory does not know its own name—that is stored in its parent. Therefore, it uses `..` to move up to
        // the root and searches each parent for an entry that points to the directory one level below
        while curr_cluster != self.superblock.root_cluster {
            // In a corrupted file system, `..` can lead to a loop
            if names.len() >= self.superblock.cluster_count as usize {
                return Err(FsError::Corrupted("cycle in directory tree".to_owned()));
            }

            let (parent_dir, entry) = self.dir_of(curr_cluster)?;

            names.push(entry.name.to_string());
            curr_cluster = parent_dir.get_cluster();
        }

        // Jména jsou posbíraná od nejhlubšího ke kořeni
        names.reverse();
        Ok(format!("/{}", names.join("/")))
    }

    pub fn dir_of(&self, cluster: u32) -> Result<(Dir, DirEntry), FsError> {
        let parent = self.get_parent_dir_cluster(cluster)?;
        let parent_dir = self.get_dir(parent)?;

        let Some(entry) = parent_dir
            .get_slots()
            .find(|entry| entry.kind == DirEntryKind::Dir && entry.start == FATPtrEntry::Next(cluster))
            .cloned()
        else {
            return Err(FsError::Corrupted(format!("directory cluster {cluster} not found in its parent")));
        };

        Ok((parent_dir, entry))
    }

    pub fn get_parent_dir_and_name_from_path(&self, path: &str) -> Result<(Dir, Name), FsError> {
        let (parent, Some(name)) = self.walk(path)? else {
            // `/`, `.`, `..` have no name to create, they always exist
            return Err(FsError::FileExist);
        };
        Ok((self.get_dir(parent)?, Name::from_str(name)?))
    }
}

impl FileSystem {
    /// Returns the directory cluster where the translation ended and the last part of the path, if it is a common name
    fn walk<'p>(&self, path: &'p str) -> Result<(u32, Option<&'p str>), FsError> {
        let mut cluster = if path.starts_with('/') { self.superblock.root_cluster } else { self.cwd };

        // Ignore empty spaces
        let mut parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();

        // We'll let the caller do whatever they want with the last part; only `.`, `..` resolved immediately to `None`.
        let last = parts.last().copied().filter(|&part| part != "." && part != "..");
        if last.is_some() {
            parts.pop();
        }

        for part in parts {
            cluster = match part {
                "." => cluster,
                ".." => self.get_parent_dir_cluster(cluster)?,
                name => self.get_child_dir_cluster(cluster, name)?,
            };
        }

        // Intermediate parts must exist as directories (/a/b/c → a, b)
        // Last part is target of a caller operation
        // (cluster = Parent cluster number, last = name of the last part)
        Ok((cluster, last))
    }
}
