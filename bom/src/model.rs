use deku::prelude::*;

#[derive(Debug, PartialEq, DekuRead)]
#[deku(endian = "big", magic = b"BOMStore")]
pub struct StoreHeader {
    // b'BOMStore'
    // pub magic: [u8; 8],
    // Always 1
    pub version: u32,
    // Number of non-null entries in BOMBlockTable
    block_count: u32,
    // Offset to index table
    index_offset: u32,
    // Length of index table, index_offset + index_len = file_length
    index_len: u32,
    // Offset to var table
    var_offset: u32,
    // Length of var table, var_offset + var_len = file_length
    var_len: u32,
    // unknown: [u8; 480],
    #[deku(seek_from_start = "*index_offset as u64")]
    pub index_store: IndexStore,
    #[deku(seek_from_start = "*var_offset as u64")]
    pub var_store: VariableStore,
}

#[derive(Debug, PartialEq, DekuRead)]
#[deku(endian = "big", ctx = "endian: deku::ctx::Endian")]
pub struct IndexStore {
    count: u32,
    #[deku(count = "count")]
    pub indexs: Vec<Index>,
}

#[derive(Debug, PartialEq, DekuRead)]
#[deku(endian = "big", ctx = "endian: deku::ctx::Endian")]
pub struct Index {
    pub offset: u32,
    pub len: u32,
}

#[derive(Debug, PartialEq, DekuRead)]
#[deku(endian = "big", ctx = "endian: deku::ctx::Endian")]
pub struct VariableStore {
    count: u32,
    #[deku(count = "count")]
    pub vars: Vec<Variable>,
}

#[derive(Debug, PartialEq, DekuRead)]
#[deku(endian = "big", ctx = "endian: deku::ctx::Endian")]
pub struct Variable {
    pub index: u32,
    pub len: u8,
    #[deku(count = "len")]
    pub name: Vec<u8>,
}

impl StoreHeader {
    pub fn index_with_name(&self, name: &[u8]) -> Option<&Index> {
        self.var_with_name(name)
            .and_then(|var| self.index_store.indexs.get(var.index as usize))
    }

    pub fn var_with_name(&self, name: &[u8]) -> Option<&Variable> {
        self.var_store.vars.iter().find(|var| var.name == name)
    }
}

#[derive(Debug, PartialEq, DekuRead)]
#[deku(endian = "big", magic = b"tree")]
pub struct TreeHeader {
    // // tree
    // tag: [u8; 4],
    // always 1
    pub version: u32,
    // Index for BOMPaths
    pub index: u32,
    // Always 4096
    pub block_size: u32,
    // Total number of paths in all leaves combined
    pub path_count: u32,
    unknown: u8,
}

#[derive(Debug, PartialEq, DekuRead)]
#[deku(endian = "big")]
pub struct TreePaths {
    pub is_leaf: u16,
    pub count: u16,
    pub forward: u32,
    pub backward: u32,
    #[deku(count = "count")]
    pub indices: Vec<TreePathIndex>,
}

#[derive(Debug, PartialEq, DekuRead)]
#[deku(endian = "big", ctx = "endian: deku::ctx::Endian")]
pub struct TreePathIndex {
    pub val: u32,
    pub key: u32,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, deku::DekuRead)]
pub struct BOMStr {
    #[deku(read_all, map = "crate::deku_read_str")]
    pub content: String,
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, deku::DekuRead)]
pub struct BOMBytes {
    #[deku(read_all)]
    pub content: Vec<u8>,
}
