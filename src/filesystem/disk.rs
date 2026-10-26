//! Lowest layer: read/write only bytes

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::filesystem::FsError;

#[derive(Debug)]
pub struct Disk {
    file: File,
}

impl Disk {
    pub fn create(path: &Path, size: u64) -> io::Result<Disk> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        // The extension will pad with zeros. On Linux, this will result in a sparse file.
        file.set_len(size)?;

        Ok(Disk { file })
    }

    pub fn open(path: &Path) -> io::Result<Disk> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Disk { file })
    }

    pub fn get_size(&self) -> io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }

    pub fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), FsError> {
        let mut file = &self.file;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buf)?;
        Ok(())
    }

    pub fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<(), FsError> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(data).map_err(|e| e.into())
    }
}
