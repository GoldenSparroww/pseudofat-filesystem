//! Command `rm`

use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
};

impl FileSystem {
    pub fn do_rm(&mut self, path: &str) -> Result<Output, CommandExecError> {
        // A directory is FILE NOT FOUND as well (rmdir is for directories)
        let (mut parent, entry) = self.get_file_from_path(path)?;

        self.remove_file(&mut parent, &entry)?;

        Ok(Output::Ok)
    }
}
