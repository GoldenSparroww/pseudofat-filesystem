//! Command `cd`

use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
};

impl FileSystem {
    pub fn do_cd(&mut self, path: &str) -> Result<Output, CommandExecError> {
        let new_cwd = self.get_dir_cluster_from_path(path)?;
        self.set_cwd(new_cwd);
        Ok(Output::Ok)
    }
}
