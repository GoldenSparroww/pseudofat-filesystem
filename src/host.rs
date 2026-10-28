use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::{fs, io};

use crate::filesystem::FsError;

/// OS specific string to PathBuf
pub fn parse_host_path(text: impl AsRef<OsStr>) -> PathBuf {
    PathBuf::from(text.as_ref())
}

/// Reads the entire file on the host and returns it as an array of bytes
pub fn read_file(path: &PathBuf) -> Result<Vec<u8>, FsError> {
    if path.is_dir() {
        return Err(FsError::HostFileNotFound);
    }

    fs::read(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => FsError::HostFileNotFound,
        _ => FsError::HostFileReadGeneralError,
    })
}

/// Creates a new file on the host with the contents of `data`
pub fn write_file(path: &PathBuf, data: &[u8]) -> Result<(), FsError> {
    if path.is_dir() {
        return Err(FsError::HostFileExists);
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| match e.kind() {
            io::ErrorKind::AlreadyExists => FsError::FileExist,
            io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => FsError::PathNotFound,
            _ => FsError::HostFileWriteGeneralError,
        })?;

    if file.write_all(data).is_err() {
        // In some cases, you cannot delete opened file, so drop it before removal
        drop(file);
        let _ = fs::remove_file(path);
        return Err(FsError::HostFileWriteGeneralError);
    }

    Ok(())
}
