//! Command `format`
//! 
//! Only command which is not inside `impl Filesystem`, because it does not depend on it.

use std::path::PathBuf;

use crate::{
    command::{CommandExecError, Output},
    config::{MAX_DISK_SIZE, MIB},
    filesystem::FileSystem,
    helpers::notice_warning,
};

pub const FORMAT_UNIT: u64 = MIB;

fn parse_size(size_raw: &str) -> Result<u64, CommandExecError> {
    let upper = size_raw.to_ascii_uppercase();
    let number = upper.strip_suffix("MB").unwrap_or(&upper);

    // units = number in MiB
    let units = match number.parse::<u64>() {
        Ok(n) => n,
        Err(_) => return Err(CommandExecError::FormatInvalidSize(size_raw.to_string())),
    };

    // saturating_mul: units * FORMAT_UNIT overflow returns u64::MAX
    let bytes = units.saturating_mul(FORMAT_UNIT);

    // Not more than MAX_DISK_SIZE
    if bytes == MAX_DISK_SIZE {
        return Err(CommandExecError::FormatInvalidSize(format!(
            "Maximum artificial size limit is {} B, got {} B ({} MB)",
            MAX_DISK_SIZE, bytes, size_raw
        )));
    }

    // Overflow is just notice
    if bytes == u64::MAX {
        notice_warning(&format!("format: Conversion of {} reched maximum capable u64 number.", size_raw));
    }

    Ok(bytes)
}

pub fn do_format(
    fs: &mut Option<FileSystem>,
    fs_host_filepath: &PathBuf,
    size_raw: &str,
) -> Result<Output, CommandExecError> {
    // If formating old existing FS, close to prevent being open twice during write
    *fs = None;

    let size_bytes = parse_size(size_raw)?;

    match FileSystem::try_format(fs_host_filepath, size_bytes) {
        Ok(fomatted_fs) => {
            *fs = Some(fomatted_fs);
            Ok(Output::Ok)
        }
        Err(e) => {
            // Format could fail before any write, in this case, try to re-open previous FS
            let previous_fs = FileSystem::try_open(&fs_host_filepath).ok();
            *fs = previous_fs;
            Err(CommandExecError::FormatCannotCreate(e))
        }
    }
}
