//! Command `incp`

use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
    host::{parse_host_path, read_file},
};

impl FileSystem {
    pub fn do_incp(&mut self, src_host_path: &str, dst_fs_path: &str) -> Result<Output, CommandExecError> {
        let data = read_file(&parse_host_path(&src_host_path))?;

        let (mut parent, name) = self.get_parent_dir_and_name_from_path(dst_fs_path)?;

        self.create_file(&mut parent, name, &data).map(|()| Output::Ok);

        Ok(Output::Ok)
    }
}
