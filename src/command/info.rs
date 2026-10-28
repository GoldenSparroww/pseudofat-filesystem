//! Command `info`

use crate::{
    command::{CommandExecError, Output},
    filesystem::{DirEntryKind, FileSystem},
};

impl FileSystem {
    pub fn do_info(&self, path: &str) -> Result<Output, CommandExecError> {
        // `None` is the root, it has no entry (and no name) in any directory
        let Some((_, entry)) = self.get_parent_dir_and_node_from_path(path)? else {
            return Ok(Output::RawText(format!("/ – dir – cluster - {}", self.get_root_cluster())));
        };

        let line = match entry.kind {
            DirEntryKind::Dir => {
                let cluster = self
                    .get_dir_cluster(&entry)
                    .map_or("?".to_owned(), |cluster| cluster.to_string());
                format!("{} – dir – cluster - {cluster}", entry.name)
            }
            DirEntryKind::File { size } => {
                // Shows the real state of FAT, so a size/chain mismatch (e.g. after `bug`) is reported, not refused
                let clusters = self.get_file_clusters(&entry)?;
                let list = match clusters.is_empty() {
                    true => "(no clusters)".to_owned(),
                    false => clusters.iter().map(u32::to_string).collect::<Vec<_>>().join(","),
                };

                let needed = (size as usize).div_ceil(self.get_cluster_size() as usize);
                let mismatch = match clusters.len() == needed {
                    true => String::new(),
                    false => format!(" (MISMATCH: size needs {needed} clusters, chain has {})", clusters.len()),
                };

                // Space actually taken on disk: whole clusters of the real FAT chain
                let on_disk = clusters.len() as u64 * self.get_cluster_size() as u64;

                format!("{} – file – size: {size}B, on disk: {on_disk}B - {list}{mismatch}", entry.name)
            }
        };

        Ok(Output::RawText(line))
    }
}
