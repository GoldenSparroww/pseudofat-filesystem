//! Command `cat`

use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
};

impl FileSystem {
    pub fn do_cat(&self, path: &str) -> Result<Output, CommandExecError> {
        let entry = self.get_file_entry_from_path(path)?; // FILE NOT FOUND
        let data = self.read_file(&entry)?;

        // Binary content would not be valid UTF-8, invalid bytes are shown as U+FFFD
        Ok(Output::RawText(String::from_utf8_lossy(&data).into_owned()))
    }
}
