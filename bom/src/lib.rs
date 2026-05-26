//! Low-level parser for Apple BOM stores.
//!
//! The stable crate root intentionally stays small: use [`BOM`] to open a store
//! and traverse blocks and trees, and use [`raw`] when you need the `deku`
//! structs that mirror on-disk layout.

mod bom;
mod model;
pub mod raw;

pub use crate::bom::{BOM, BOMBlock, BOMEror, BOMResult, ByteSlice, ByteSource};
