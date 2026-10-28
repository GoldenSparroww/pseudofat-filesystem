//! Command `load`
//!
//! Only reads the script from our FS. The lines are executed by the shell, because only the shell
//! knows how to run a command (and print its result). It is expected to get the script before exectution
//! via `incp` command from host.

use crate::{
    command::CommandExecError,
    filesystem::FileSystem,
    shell::{Flow, Shell},
};

impl FileSystem {
    /// Returns the lines of the script file `path` (the file has to be copied in by `incp` first)
    pub fn file_to_lines(&self, path: &str) -> Result<Vec<String>, CommandExecError> {
        let entry = self.get_file_entry_from_path(path)?; // FILE NOT FOUND
        let data = self.read_file(&entry)?;

        // `lines()` handles both LF and CRLF
        Ok(String::from_utf8_lossy(&data).lines().map(str::to_owned).collect())
    }
}

impl Shell {
    /// Executes the script read by `file_to_lines` line by line. A failed command does not stop the rest.
    /// Returns `Flow::Exit` if the script contains `exit` (it ends the whole program)
    pub fn do_load(&mut self, path: &str) -> Result<Flow, CommandExecError> {
        if self.is_load_running() {
            return Err(CommandExecError::NotAllowed("nested load".to_owned()));
        }

        let lines = self.require_fs()?.file_to_lines(path)?;

        self.set_load_running(true);
        let mut flow = Flow::Continue;
        for line in &lines {
            if let Flow::Exit = self.process_line(line) {
                flow = Flow::Exit;
                break;
            }
        }
        self.set_load_running(false);

        Ok(flow)
    }
}
