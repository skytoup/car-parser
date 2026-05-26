pub mod rendition;

use std::io::{Cursor, Read, Seek};

use bom::{BOMBlock, BOMEror, BOMResult};
use deku::{ctx::Order, prelude::*, reader::Reader};

// 扩展源信息
#[derive(Debug, DekuRead)]
#[deku(endian = "big", magic = b"META")]
pub struct ExtendedMetadata {
    #[deku(bytes_read = "256", map = "crate::deku_read_str")]
    pub thinning_args: String,
    #[deku(bytes_read = "256", map = "crate::deku_read_str")]
    pub deployment_platform_version: String,
    #[deku(bytes_read = "256", map = "crate::deku_read_str")]
    pub deployment_platform: String,
    #[deku(bytes_read = "256", map = "crate::deku_read_str")]
    pub authoring_tool: String,
}

#[derive(Debug, DekuRead)]
#[deku(magic = b"RATC", endian = "little")]
pub struct Header {
    pub coreui_version: u32,
    pub storage_version: u32,
    pub storage_timestamp: u32,
    pub rendition_count: u32,
    #[deku(bytes_read = "128", map = "crate::deku_read_str")]
    pub main_version_string: String,
    #[deku(bytes_read = "256", map = "crate::deku_read_str")]
    pub version_string: String,
    pub uuid: [u8; 16],
    pub associated_checksumag: u32,
    pub schema_version: u32,
    pub color_space: ColorSpace,
    pub key_semantics: u32,
}

#[derive(Debug)]
pub struct CSIHeader {
    pub version: u32,
    pub flags: rendition::Flags,
    pub width: u32,
    pub height: u32,
    // 100 to @1x, 200 to @2x, 300 to @3x
    pub scale_factor: u32,
    pub encoding: Encoding,
    pub color_model: ColorModel,
    pub metadata: CSIMetadata,
    pub tlv_length: u32,
    pub bitmap_list: rendition::BitmapList,
    pub tlv_data: Vec<rendition::RenditionType>,
    pub rendition: Option<rendition::Rendition>,
}

impl CSIHeader {
    pub(crate) fn from_bom_block(block: BOMBlock) -> BOMResult<Self> {
        let mut reader = Reader::new(block);
        let parts = read_csi_header_parts(&mut reader)?;
        let tlv_data = read_tlv_data_from_bom(&mut reader, parts.tlv_length)?;
        let rendition = if parts.bitmap_list.rendition_length > 0 {
            let payload = payload_slice(&mut reader, parts.bitmap_list.rendition_length)?;
            Some(rendition::parse_rendition_payload(payload)?)
        } else {
            None
        };

        Ok(Self::from_parts(parts, tlv_data, rendition))
    }

    fn from_parts(
        parts: CSIHeaderParts,
        tlv_data: Vec<rendition::RenditionType>,
        rendition: Option<rendition::Rendition>,
    ) -> Self {
        Self {
            version: parts.version,
            flags: parts.flags,
            width: parts.width,
            height: parts.height,
            scale_factor: parts.scale_factor,
            encoding: parts.encoding,
            color_model: parts.color_model,
            metadata: parts.metadata,
            tlv_length: parts.tlv_length,
            bitmap_list: parts.bitmap_list,
            tlv_data,
            rendition,
        }
    }

    pub fn size_on_disk(&self) -> usize {
        14 * 4 + 128 + self.tlv_length as usize + self.bitmap_list.rendition_length as usize
    }
}

impl<'a> DekuReader<'a> for CSIHeader {
    fn from_reader_with_ctx<R: Read + Seek>(
        reader: &mut Reader<R>,
        _ctx: (),
    ) -> Result<Self, DekuError>
    where
        Self: Sized,
    {
        let parts = read_csi_header_parts(reader)?;
        let tlv_data = read_tlv_data(reader, parts.tlv_length)?;
        let rendition = if parts.bitmap_list.rendition_length > 0 {
            let payload = read_payload_bytes(reader, parts.bitmap_list.rendition_length)?;
            Some(
                rendition::parse_rendition_payload(crate::PayloadBytes::from_vec(payload))
                    .map_err(bom_error_to_deku)?,
            )
        } else {
            None
        };

        Ok(Self::from_parts(parts, tlv_data, rendition))
    }
}

struct CSIHeaderParts {
    version: u32,
    flags: rendition::Flags,
    width: u32,
    height: u32,
    scale_factor: u32,
    encoding: Encoding,
    color_model: ColorModel,
    metadata: CSIMetadata,
    tlv_length: u32,
    bitmap_list: rendition::BitmapList,
}

fn read_csi_header_parts<R: Read + Seek>(
    reader: &mut Reader<R>,
) -> Result<CSIHeaderParts, DekuError> {
    read_magic(reader, b"ISTC")?;
    Ok(CSIHeaderParts {
        version: read_u32(reader)?,
        flags: rendition::Flags::from_reader_with_ctx(reader, deku::ctx::Endian::Little)?,
        width: read_u32(reader)?,
        height: read_u32(reader)?,
        scale_factor: read_u32(reader)?,
        encoding: Encoding::from_reader_with_ctx(reader, deku::ctx::Endian::Little)?,
        color_model: ColorModel::from_reader_with_ctx(reader, deku::ctx::Endian::Little)?,
        metadata: CSIMetadata::from_reader_with_ctx(reader, deku::ctx::Endian::Little)?,
        tlv_length: read_u32(reader)?,
        bitmap_list: rendition::BitmapList::from_reader_with_ctx(
            reader,
            deku::ctx::Endian::Little,
        )?,
    })
}

fn read_magic<R: Read + Seek>(reader: &mut Reader<R>, expected: &[u8; 4]) -> Result<(), DekuError> {
    let mut actual = [0_u8; 4];
    reader.read_bytes(4, &mut actual, Order::default())?;
    if &actual != expected {
        return Err(DekuError::Parse(
            format!(
                "invalid CSI header magic: expected {:?}, got {:?}",
                expected, actual
            )
            .into(),
        ));
    }
    Ok(())
}

fn read_u32<R: Read + Seek>(reader: &mut Reader<R>) -> Result<u32, DekuError> {
    u32::from_reader_with_ctx(reader, deku::ctx::Endian::Little)
}

fn read_payload_bytes<R: Read + Seek>(
    reader: &mut Reader<R>,
    len: u32,
) -> Result<Vec<u8>, DekuError> {
    let len = len as usize;
    let mut bytes = vec![0_u8; len];
    reader.read_bytes(len, &mut bytes, Order::default())?;
    Ok(bytes)
}

fn payload_slice(reader: &mut Reader<BOMBlock>, len: u32) -> BOMResult<crate::PayloadBytes> {
    let len = len as usize;
    let slice = reader.as_mut().slice_at_current(len)?;
    reader.bits_read = reader.bits_read.saturating_add(len.saturating_mul(8));
    reader.leftover = None;
    Ok(slice)
}

fn read_tlv_data<R: Read + Seek>(
    reader: &mut Reader<R>,
    len: u32,
) -> Result<Vec<rendition::RenditionType>, DekuError> {
    let bytes = read_payload_bytes(reader, len)?;
    parse_tlv_data(&bytes)
}

fn read_tlv_data_from_bom(
    reader: &mut Reader<BOMBlock>,
    len: u32,
) -> BOMResult<Vec<rendition::RenditionType>> {
    let bytes = payload_slice(reader, len)?;
    parse_tlv_data(bytes.as_slice()).map_err(BOMEror::from)
}

fn parse_tlv_data(bytes: &[u8]) -> Result<Vec<rendition::RenditionType>, DekuError> {
    let mut cursor = Cursor::new(bytes);
    let mut reader = Reader::new(&mut cursor);
    let mut result = Vec::new();
    while (reader.as_mut().position() as usize) < bytes.len() {
        result.push(rendition::RenditionType::from_reader_with_ctx(
            &mut reader,
            deku::ctx::Endian::Little,
        )?);
    }
    Ok(result)
}

fn bom_error_to_deku(err: BOMEror) -> DekuError {
    match err {
        BOMEror::ParseStruct(err) => err,
        err => DekuError::Parse(err.to_string().into()),
    }
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct CSIMetadata {
    pub modification_time: u32,
    pub layout_type: rendition::LayoutType,
    #[deku(bytes_read = "128", map = "crate::deku_read_str")]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, DekuRead)]
#[deku(id_type = "u32", endian = "little", ctx = "endian: deku::ctx::Endian")]
pub enum ColorSpace {
    #[deku(id = 1)]
    SRGB,
    #[deku(id = 2)]
    GrayGamma2_2,
    #[deku(id = 3)]
    DisplayP3,
    #[deku(id = 4)]
    ExtendedRangeSRGB,
    #[deku(id = 5)]
    ExtendedLinearSRGB,
    #[deku(id = 6)]
    ExtendedGray,

    #[deku(id = 257)]
    SystemSRGB,

    #[deku(id_pat = "_")]
    Unknown { tag: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, DekuRead)]
#[deku(id_type = "u32", endian = "little", ctx = "endian: deku::ctx::Endian")]
pub enum ColorModel {
    #[deku(id = 0)]
    None,
    #[deku(id = 1)]
    RGB,
    #[deku(id = 2)]
    Monochrome,
    #[deku(id = 3)]
    RGB0, // unknown
    #[deku(id = 4)]
    RGBP3,

    #[deku(id_pat = "_")]
    Unknown { tag: u32 },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, DekuRead)]
#[deku(
    id_type = "[u8; 4]",
    endian = "little",
    ctx = "endian: deku::ctx::Endian"
)]
pub enum Encoding {
    #[deku(id = "[0, 0, 0, 0]")]
    None,
    #[deku(id = b"BGRA")]
    ARGB,
    #[deku(id = b"ATAD")]
    Data,
    #[deku(id = b"YARG")]
    GRAY,
    #[deku(id = b"GEPJ")]
    JPEG,
    #[deku(id = b" FDP")]
    PDF,
    #[deku(id = b"PBEW")]
    WEBP,
    #[deku(id = b"WBGR")]
    ARGB16,
    #[deku(id = b"61AG")]
    GA16,
    #[deku(id = b" 8AG")]
    GA8,
    #[deku(id = b"5BGR")]
    RGB5,
    #[deku(id = b" GVS")]
    SVG,
    #[deku(id = b"FIEH")]
    HEIF,

    #[deku(id_pat = "_")]
    Unknown { tag: [u8; 4] },
}
