//! Převody čísel a polí bajtů do bufferů a zpět
//!
//! Struktury na disku (superblok, položka adresáře) se skládají ručně: každé pole má pevnou
//! pozici a čísla jsou uložená jako little-endian
//!
//! Indexování může zpanikařit jen při chybě v programu (špatný offset), nikdy kvůli datům na
//! disku – offsety jsou konstanty uvnitř bufferu pevné velikosti

/// Zapíše bajty `value` na pozici `at`
pub fn put_bytes(bytes: &mut [u8], at: usize, value: &[u8]) {
    bytes[at..at + value.len()].copy_from_slice(value);
}

/// Přečte `N` bajtů z pozice `at` jako pole `[u8; N]`, `N` se odvodí z místa použití
pub fn get_bytes<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
    let mut buf = [0u8; N];
    buf.copy_from_slice(&bytes[at..at + N]);
    buf
}

/// Zapíše `value` jako 4 bajty little-endian na pozici `at`
pub fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    put_bytes(bytes, at, &value.to_le_bytes());
}

/// Zapíše `value` jako 8 bajtů little-endian na pozici `at`
pub fn put_u64(bytes: &mut [u8], at: usize, value: u64) {
    put_bytes(bytes, at, &value.to_le_bytes());
}

/// Přečte 4 bajty little-endian z pozice `at`
pub fn get_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(get_bytes(bytes, at))
}

/// Přečte 8 bajtů little-endian z pozice `at`
pub fn get_u64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(get_bytes(bytes, at))
}
