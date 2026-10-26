use std::ffi::OsStr;
use std::path::PathBuf;

/// OS specific string to PathBuf
pub fn parse_host_path(text: impl AsRef<OsStr>) -> PathBuf {
    PathBuf::from(text.as_ref())
}
