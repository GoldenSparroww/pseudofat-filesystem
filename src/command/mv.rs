//! Command `mv`

use crate::{
    command::{CommandExecError, Output},
    filesystem::{FileSystem, FsError},
};

impl FileSystem {
    /// Moves or renames a file or a directory. `dst` is always the full target path including the new name
    pub fn do_mv(&mut self, src: &str, dst: &str) -> Result<Output, CommandExecError> {
        let Some((mut parent_dir, entry_to_move)) = self.get_parent_dir_and_node_from_path(src)? else {
            return Err(CommandExecError::NotAllowed("cannot move the root directory".to_owned()));
        };

        let (mut target_dir, new_name) = self.get_parent_dir_and_name_from_path(dst)?; // PATH NOT FOUND

        // A directory moved into its own subtree would be disconnected from the root
        if let Some(cluster) = self.get_dir_cluster(&entry_to_move)
            && self.is_dir_inside(target_dir.get_cluster(), cluster)?
        {
            return Err(CommandExecError::NotAllowed("cannot move a directory into itself".to_owned()));
        }

        // Moving onto itself (`mv a a`) is an existing target as well
        if target_dir.find(&new_name).is_some() {
            return Err(FsError::FileExist.into());
        }

        self.move_entry(&mut parent_dir, &entry_to_move, &mut target_dir, new_name)?;

        Ok(Output::Ok)
    }
}
