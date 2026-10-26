//! Command `ls`

use crate::filesystem::{DirEntry, DirEntryKind};
use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
};

impl FileSystem {
    pub fn do_ls(&self, path: Option<&str>) -> Result<Output, CommandExecError> {
        let cluster = match path {
            Some(path) => self.get_dir_cluster_from_path(path)?,
            None => self.get_cwd(),
        };

        let dir = self.get_dir(cluster)?;
        let lines: Vec<String> = dir.get_slots().map(ls_line).collect();
        Ok(Output::RawText(lines.join("\n")))
    }
}

fn ls_line(entry: &DirEntry) -> String {
    match entry.kind {
        DirEntryKind::File { .. } => format!("FILE: {}", entry.name),
        DirEntryKind::Dir => format!("DIR: {}", entry.name),
    }
}
