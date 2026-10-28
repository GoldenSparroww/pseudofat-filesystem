//! Command `check`

use crate::{
    command::{CommandExecError, Output},
    filesystem::FileSystem,
};

impl FileSystem {
    pub fn do_check(&self) -> Result<Output, CommandExecError> {
        let problems = self.check()?;

        match problems.is_empty() {
            true => Ok(Output::Ok),
            false => Ok(Output::RawText(problems.join("\n"))),
        }
    }
}
