//! Command `mkdir`

use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
};

impl FileSystem {
    pub fn do_mkdir(&mut self, path: &str) -> Result<Output, CommandExecError> {
        let (mut parent, name) = self.get_parent_dir_and_name_from_path(path)?;

        self.create_dir(&mut parent, name)?;

        Ok(Output::Ok)
    }
}
