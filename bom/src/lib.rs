//! Low-level parser for Apple BOM stores.
//!
//! The stable crate root intentionally stays small: use [`BOM`] to open a store
//! and traverse blocks and trees, and use [`raw`] when you need the `deku`
//! structs that mirror on-disk layout.

mod bom;
mod model;
pub mod raw;

pub use crate::bom::{BOM, BOMBlock, BOMEror, BOMResult, ByteSlice, ByteSource};

pub(crate) fn deku_read_str(bytes: Vec<u8>) -> Result<String, deku::DekuError> {
    Ok(String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .to_string())
}
