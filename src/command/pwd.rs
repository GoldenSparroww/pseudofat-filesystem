//! Command `pwd`

use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
};

impl FileSystem {
    pub fn do_pwd(&self) -> Result<Output, CommandExecError> {
        let path = self.get_path_from_cluster(self.get_cwd())?;
        Ok(Output::RawText(path))
    }
}
