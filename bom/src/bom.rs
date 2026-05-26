use thiserror::{self, Error};

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    fs::File,
    hash::Hash,
    io::{self, Read, Seek, SeekFrom},
    ops::{Deref, Range},
    path::Path,
    sync::Arc,
};

use deku::{DekuContainerRead, reader::Reader};
use memmap2::Mmap;

pub use crate::model::*;

pub type BOMResult<T> = Result<T, BOMEror>;

#[derive(Clone)]
pub struct ByteSource {
    inner: Arc<ByteSourceInner>,
}

enum ByteSourceInner {
    Owned(Box<[u8]>),
    Mmap(Mmap),
}

impl ByteSource {
    pub fn from_vec(bytes: Vec<u8>) -> Self {
        Self::from_boxed_slice(bytes.into_boxed_slice())
    }

    pub fn from_boxed_slice(bytes: Box<[u8]>) -> Self {
        Self {
            inner: Arc::new(ByteSourceInner::Owned(bytes)),
        }
    }

    pub fn from_mmap(mmap: Mmap) -> Self {
        Self {
            inner: Arc::new(ByteSourceInner::Mmap(mmap)),
        }
    }

    pub fn from_reader<R>(mut reader: R) -> io::Result<Self>
    where
        R: Read,
    {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(Self::from_vec(bytes))
    }

    pub fn as_slice(&self) -> &[u8] {
        match self.inner.as_ref() {
            ByteSourceInner::Owned(bytes) => bytes,
            ByteSourceInner::Mmap(mmap) => mmap,
        }
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    pub fn slice(&self, range: Range<usize>) -> BOMResult<ByteSlice> {
        if range.start > range.end || range.end > self.len() {
            return Err(BOMEror::InvalidByteRange {
                offset: range.start,
                len: range.end.saturating_sub(range.start),
                source_len: self.len(),
            });
        }

        Ok(ByteSlice {
            source: self.clone(),
            range,
        })
    }
}

impl fmt::Debug for ByteSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ByteSource")
            .field("len", &self.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct ByteSlice {
    source: ByteSource,
    range: Range<usize>,
}

impl ByteSlice {
    pub fn from_vec(bytes: Vec<u8>) -> Self {
        Self::from_boxed_slice(bytes.into_boxed_slice())
    }

    pub fn from_boxed_slice(bytes: Box<[u8]>) -> Self {
        let source = ByteSource::from_boxed_slice(bytes);
        source
            .slice(0..source.len())
            .expect("full byte source range should be valid")
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.source.as_slice()[self.range.clone()]
    }

    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.as_slice().to_vec()
    }

    pub fn absolute_range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub fn slice(&self, range: Range<usize>) -> BOMResult<Self> {
        if range.start > range.end || range.end > self.len() {
            return Err(BOMEror::InvalidByteRange {
                offset: self.range.start.saturating_add(range.start),
                len: range.end.saturating_sub(range.start),
                source_len: self.source.len(),
            });
        }

        let start = self.range.start + range.start;
        let end = self.range.start + range.end;
        self.source.slice(start..end)
    }
}

impl Deref for ByteSlice {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl fmt::Debug for ByteSlice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ByteSlice")
            .field("range", &self.range)
            .field("len", &self.len())
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct BOMBlock {
    bytes: ByteSlice,
    position: u64,
}

impl BOMBlock {
    pub fn new(bytes: ByteSlice) -> Self {
        Self { bytes, position: 0 }
    }

    pub fn as_slice(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    pub fn byte_slice(&self) -> &ByteSlice {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn slice_at_current(&mut self, len: usize) -> BOMResult<ByteSlice> {
        let start = usize::try_from(self.position).map_err(|_| BOMEror::InvalidByteRange {
            offset: usize::MAX,
            len,
            source_len: self.bytes.len(),
        })?;
        let end = start.checked_add(len).ok_or(BOMEror::InvalidByteRange {
            offset: start,
            len,
            source_len: self.bytes.len(),
        })?;
        let slice = self.bytes.slice(start..end)?;
        self.position = end as u64;
        Ok(slice)
    }
}

impl Read for BOMBlock {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let pos = usize::try_from(self.position).unwrap_or(usize::MAX);
        let data = self.bytes.as_slice();
        if pos >= data.len() {
            return Ok(0);
        }

        let amt = buf.len().min(data.len() - pos);
        buf[..amt].copy_from_slice(&data[pos..pos + amt]);
        self.position += amt as u64;
        Ok(amt)
    }
}

impl Seek for BOMBlock {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let len = self.bytes.len() as i128;
        let current = self.position as i128;
        let next = match pos {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::End(offset) => len.checked_add(offset as i128).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "seek position overflow")
            })?,
            SeekFrom::Current(offset) => current.checked_add(offset as i128).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "seek position overflow")
            })?,
        };

        if next < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek before start of BOM block",
            ));
        }

        self.position = next as u64;
        Ok(self.position)
    }
}

pub struct BOM {
    source: ByteSource,
    store_header: StoreHeader,
}

impl BOM {
    pub fn new<R>(mut reader: R) -> BOMResult<Self>
    where
        R: Read + Seek,
    {
        reader.seek(SeekFrom::Start(0))?;
        let source = ByteSource::from_reader(reader)?;
        Self::from_byte_source(source)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> BOMResult<Self> {
        Self::from_byte_source(ByteSource::from_vec(bytes))
    }

    pub fn from_boxed_slice(bytes: Box<[u8]>) -> BOMResult<Self> {
        Self::from_byte_source(ByteSource::from_boxed_slice(bytes))
    }

    pub fn from_byte_source(source: ByteSource) -> BOMResult<Self> {
        let full = source.slice(0..source.len())?;
        let mut block = BOMBlock::new(full);
        let (_, store_header) = StoreHeader::from_reader((&mut block, 0))?;
        Ok(BOM {
            source,
            store_header,
        })
    }

    pub fn source(&self) -> &ByteSource {
        &self.source
    }

    fn block_with_name(&mut self, name: &[u8]) -> BOMResult<BOMBlock> {
        let var = self
            .store_header
            .var_with_name(name)
            .ok_or(BOMEror::NotFoundVar(
                String::from_utf8_lossy(name).to_string(),
            ))?;

        self.block_at(var.index as usize)
    }

    fn block_at(&mut self, index: usize) -> BOMResult<BOMBlock> {
        let idx = self.store_header.index_store.indexs.get(index);
        if let Some(idx) = idx {
            let offset = idx.offset as usize;
            let len = idx.len as usize;
            let end = offset.checked_add(len).ok_or(BOMEror::InvalidIndexRange {
                index,
                offset,
                len,
                source_len: self.source.len(),
            })?;
            if end > self.source.len() {
                return Err(BOMEror::InvalidIndexRange {
                    index,
                    offset,
                    len,
                    source_len: self.source.len(),
                });
            }

            return self.source.slice(offset..end).map(BOMBlock::new);
        }

        Err(BOMEror::NotFoundIndex(index))
    }

    fn tree_with_name(&mut self, name: &[u8]) -> BOMResult<Vec<TreePaths>> {
        let mut block = self.block_with_name(name)?;
        let (_, header) = TreeHeader::from_reader((&mut block, 0))?;

        let mut tree_paths = vec![];
        let mut tree_idx = header.index;
        loop {
            let path: TreePaths = self.read_block_at(tree_idx as usize)?;
            if path.is_leaf == 0 {
                if let Some(idx) = path.indices.first() {
                    tree_idx = idx.val;
                    continue;
                }
                break;
            }

            let next_idx = path.forward;
            tree_paths.push(path);
            if next_idx > 0 {
                tree_idx = next_idx;
            } else {
                break;
            }
        }

        Ok(tree_paths)
    }

    pub fn read_block_at<'a, T>(&mut self, index: usize) -> BOMResult<T>
    where
        T: deku::DekuReader<'a>,
    {
        let block = self.block_at(index)?;
        let mut reader = Reader::new(block);
        let data = T::from_reader_with_ctx(&mut reader, ())?;

        Ok(data)
    }

    pub fn read_block_with_name<'a, T>(&mut self, name: &[u8]) -> BOMResult<T>
    where
        T: deku::DekuReader<'a>,
    {
        let var = self
            .store_header
            .var_with_name(name)
            .ok_or(BOMEror::NotFoundVar(
                String::from_utf8_lossy(name).to_string(),
            ))?;
        self.read_block_at(var.index as usize)
    }

    pub fn read_tree_to_btree_map<'a, K, V>(&mut self, name: &[u8]) -> BOMResult<BTreeMap<K, V>>
    where
        K: deku::DekuReader<'a> + Ord,
        V: deku::DekuReader<'a>,
    {
        let mut map = BTreeMap::new();
        self.parse_tree(name, |k, v| {
            let k = K::from_reader_with_ctx(&mut Reader::new(k), ())?;
            let v = V::from_reader_with_ctx(&mut Reader::new(v), ())?;
            map.insert(k, v);

            Ok(())
        })?;

        Ok(map)
    }

    pub fn read_tree_to_map<'a, K, V>(&mut self, name: &[u8]) -> BOMResult<HashMap<K, V>>
    where
        K: deku::DekuReader<'a> + Ord + Hash,
        V: deku::DekuReader<'a>,
    {
        let mut map = HashMap::new();
        self.parse_tree(name, |k, v| {
            let k = K::from_reader_with_ctx(&mut Reader::new(k), ())?;
            let v = V::from_reader_with_ctx(&mut Reader::new(v), ())?;
            map.insert(k, v);

            Ok(())
        })?;

        Ok(map)
    }

    pub fn parse_tree<F>(&mut self, name: &[u8], mut block: F) -> BOMResult<()>
    where
        F: FnMut(BOMBlock, BOMBlock) -> BOMResult<()>,
    {
        let paths = self.tree_with_name(name)?;
        for path in paths {
            for i in path.indices {
                let k = self.block_at(i.key as usize)?;
                let v = self.block_at(i.val as usize)?;
                block(k, v)?;
            }
        }

        Ok(())
    }
}

impl BOM {
    pub fn new_with_file<P>(file_path: P) -> BOMResult<Self>
    where
        P: AsRef<Path>,
    {
        let file = File::options().read(true).open(file_path)?;
        let mmap = unsafe { Mmap::map(&file) }?;
        Self::from_byte_source(ByteSource::from_mmap(mmap))
    }
}

#[derive(Error, Debug)]
pub enum BOMEror {
    #[error("Read failed {0}")]
    ReadIO(#[from] io::Error),
    #[error("Parse struct failed {0}")]
    ParseStruct(#[from] deku::DekuError),
    #[error("Cann't not found for index {0}")]
    NotFoundIndex(usize),
    #[error("Invalid BOM index range {index}: offset {offset}, len {len}, source len {source_len}")]
    InvalidIndexRange {
        index: usize,
        offset: usize,
        len: usize,
        source_len: usize,
    },
    #[error("Invalid byte range: offset {offset}, len {len}, source len {source_len}")]
    InvalidByteRange {
        offset: usize,
        len: usize,
        source_len: usize,
    },
    #[error("Cann't not found for name {0}")]
    NotFoundVar(String),
    #[error("Cann't not found for tree {0}")]
    NotFoundTree(String),
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::{BOM, BOMEror};

    fn push_be_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn bom_bytes_with_index(offset: u32, len: u32, payload: &[u8]) -> Vec<u8> {
        let index_offset = 32u32;
        let var_offset = 44u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"BOMStore");
        push_be_u32(&mut bytes, 1); // version
        push_be_u32(&mut bytes, 1); // block_count
        push_be_u32(&mut bytes, index_offset);
        push_be_u32(&mut bytes, 12); // index_len: count + one entry
        push_be_u32(&mut bytes, var_offset);
        push_be_u32(&mut bytes, 4); // var_len: empty variable store count
        push_be_u32(&mut bytes, 1); // index count
        push_be_u32(&mut bytes, offset);
        push_be_u32(&mut bytes, len);
        push_be_u32(&mut bytes, 0); // variable count

        let offset = offset as usize;
        if bytes.len() < offset {
            bytes.resize(offset, 0);
        }
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn block_at_returns_range_view_bytes() {
        let bytes = bom_bytes_with_index(64, 5, b"hello");
        let mut bom = BOM::from_bytes(bytes).expect("synthetic BOM should parse");

        let mut block = bom.block_at(0).expect("block should exist");
        assert_eq!(block.as_slice(), b"hello");
        assert_eq!(block.byte_slice().absolute_range(), 64..69);

        let mut read = Vec::new();
        block.read_to_end(&mut read).expect("block should read");
        assert_eq!(read, b"hello");
    }

    #[test]
    fn block_at_rejects_out_of_range_index() {
        let bytes = bom_bytes_with_index(100, 10, &[]);
        let mut bom = BOM::from_bytes(bytes).expect("synthetic BOM should parse");

        let err = bom
            .block_at(0)
            .expect_err("invalid index range should fail");
        assert!(matches!(
            err,
            BOMEror::InvalidIndexRange {
                index: 0,
                offset: 100,
                len: 10,
                ..
            }
        ));
    }
}
