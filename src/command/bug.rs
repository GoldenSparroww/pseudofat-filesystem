//! Command `bug`

use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    command::{CommandExecError, Output},
    filesystem::{BugKind, FileSystem},
};

impl FileSystem {
    /// Corrupts the file `path`. Without `kind` it picks a random kind that can be applied to the file
    pub fn do_bug(&mut self, path: &str, kind: Option<BugKind>) -> Result<Output, CommandExecError> {
        let (mut parent_dir, entry_file) = self.get_file_from_path(path)?; // FILE NOT FOUND
        let available = self.get_bug_kinds(&entry_file);

        let kind = match kind {
            Some(kind) if available.contains(&kind) => kind,
            Some(kind) => {
                return Err(CommandExecError::NotAllowed(format!(
                    "cannot apply {kind} (file has no clusters or its chain is already broken)"
                )));
            }
            None => available[random_index(available.len())],
        };

        self.bug(&mut parent_dir, &entry_file, kind)?;

        Ok(Output::RawText(format!("Successfully damaged ({kind})")))
    }
}

/// Random enough for picking a kind, no need for the `rand` crate
fn random_index(len: usize) -> usize {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |time| time.subsec_nanos());
    nanos as usize % len
}
