//! Command `rmdir`

use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
};

impl FileSystem {
    pub fn do_rmdir(&mut self, path: &str) -> Result<Output, CommandExecError> {
        let cluster = self.get_dir_cluster_from_path(path)?;

        if cluster == self.get_root_cluster() {
            return Err(CommandExecError::NotAllowed("cannot remove the root directory".to_owned()));
        }

        if self.get_dir(cluster)?.get_slots().next().is_some() {
            return Err(CommandExecError::RmdirNotEmpty);
        }

        if cluster == self.get_cwd() {
            return Err(CommandExecError::NotAllowed("cannot remove the current directory".to_owned()));
        }

        self.remove_dir(cluster)?;

        Ok(Output::Ok)
    }
}
