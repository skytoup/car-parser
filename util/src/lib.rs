pub mod apple_compression;

use deku::DekuError;

pub fn deku_read_str(bytes: Vec<u8>) -> Result<String, DekuError> {
    Ok(String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .to_string())
}
