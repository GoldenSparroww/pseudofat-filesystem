pub fn get_bytes_seq<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
    let mut buf = [0u8; N];
    buf.copy_from_slice(&bytes[at..at + N]);
    buf
}

/// Reads 4 little-endian bytes from `at`
pub fn get_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(get_bytes_seq(bytes, at))
}

/// Reads 8 little-endian bytes from `at`
pub fn get_u64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(get_bytes_seq(bytes, at))
}

/// Write bytes `value` at `at`
pub fn put_bytes_seq(bytes: &mut [u8], at: usize, value: &[u8]) {
    bytes[at..at + value.len()].copy_from_slice(value);
}

/// Write `value` as 4 little-endian bytes at `at`
pub fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    put_bytes_seq(bytes, at, &value.to_le_bytes());
}

/// Write `value` as 8 little-endian bytes at `at`
pub fn put_u64(bytes: &mut [u8], at: usize, value: u64) {
    put_bytes_seq(bytes, at, &value.to_le_bytes());
}
