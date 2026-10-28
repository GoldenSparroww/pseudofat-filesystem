//! Command `outcp`

use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
    host::{parse_host_path, write_file},
};

impl FileSystem {
    pub fn do_outcp(&mut self, src_fs_path: &str, dst_host_path: &str) -> Result<Output, CommandExecError> {
        let entry = self.get_file_entry_from_path(src_fs_path)?; // FILE NOT FOUND
        let data = self.read_file(&entry)?;
        write_file(&parse_host_path(dst_host_path), &data)?; // PATH NOT FOUND
        Ok(Output::Ok)
    }
}
