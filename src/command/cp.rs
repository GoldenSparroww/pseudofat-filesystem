//! Command `cp`

use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
};

impl FileSystem {
    pub fn do_cp(&mut self, src_fs_path: &str, dst_fs_path: &str) -> Result<Output, CommandExecError> {
        let entry = self.get_file_entry_from_path(src_fs_path)?; // FILE NOT FOUND
        let data = self.read_file(&entry)?;

        let (mut parent, name) = self.get_parent_dir_and_name_from_path(dst_fs_path)?; // PATH NOT FOUND
        self.create_file(&mut parent, name, &data)?; // EXIST

        Ok(Output::Ok)
    }
}
