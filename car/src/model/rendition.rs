use std::io::{Read, Seek, SeekFrom};

use bom::{BOMBlock, BOMEror, BOMResult};
use deku::{ctx::Order, prelude::*, reader::Reader};

use super::ColorSpace;
use crate::PayloadBytes;

#[derive(Debug, DekuRead)]
#[deku(magic = b"tmfk", endian = "little")]
pub struct KeyFmt {
    pub version: u32,
    pub max_count: u32,
    #[deku(reader = "KeyFmt::read_attr_types(deku::reader, *max_count)")]
    pub attribute_types: Vec<AttributeType>,
}

impl KeyFmt {
    fn read_attr_types<R: Read + Seek>(
        reader: &mut Reader<R>,
        count: u32,
    ) -> Result<Vec<AttributeType>, DekuError> {
        let mut attr_types: Vec<AttributeType> = vec![];
        for _ in 0..count {
            attr_types.push(AttributeType::from_reader_with_ctx(
                reader,
                deku::ctx::Endian::Little,
            )?);
            reader.seek(SeekFrom::Current(2))?;
        }
        Ok(attr_types)
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, DekuRead)]
#[deku(endian = "little")]
pub struct KeyToken {
    pub x: u16,
    pub y: u16,
    count: u16,
    #[deku(count = "count")]
    pub attrs: Vec<Attribute>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct Attribute {
    pub name: AttributeType,
    pub val: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, DekuRead)]
#[deku(id_type = "u16", endian = "little", ctx = "endian: deku::ctx::Endian")]
pub enum AttributeType {
    #[deku(id = 0)]
    ThemeLook,
    #[deku(id = 1)]
    Element,
    #[deku(id = 2)]
    Part,
    #[deku(id = 3)]
    Size,
    #[deku(id = 4)]
    Direction,
    #[deku(id = 5)]
    Placeholder,
    #[deku(id = 6)]
    Value,
    #[deku(id = 7)]
    ThemeAppearance,
    #[deku(id = 8)]
    Dimension1,
    #[deku(id = 9)]
    Dimension2,
    #[deku(id = 10)]
    State,
    #[deku(id = 11)]
    Layer,
    #[deku(id = 12)]
    Scale,
    #[deku(id = 13)]
    Localization,
    #[deku(id = 14)]
    PresentationState,
    #[deku(id = 15)]
    Idiom,
    #[deku(id = 16)]
    Subtype,
    #[deku(id = 17)]
    Identifier,
    #[deku(id = 18)]
    PreviousValue,
    #[deku(id = 19)]
    PreviousState,
    #[deku(id = 20)]
    HorizontalSizeClass,
    #[deku(id = 21)]
    VerticalSizeClass,
    #[deku(id = 22)]
    MemoryLevelClass,
    #[deku(id = 23)]
    GraphicsFeatureSetClass,
    #[deku(id = 24)]
    DisplayGamut,
    #[deku(id = 25)]
    DeploymentTarget,
    #[deku(id = 26)]
    GlyphWeight,
    #[deku(id = 27)]
    GlyphSize,

    #[deku(id_pat = "_")]
    Unknown { tag: u16 },
}

#[derive(Debug, Clone)]
pub enum Rendition {
    Color(RenditionColor),
    RawData(RenditionRawData),
    ThemeCBCK(RenditionThemeCBCK),
    MultisizeImageSet(RenditionMultisizeImageSet),
    Unknown {
        tag: [u8; 4],
        data: RenditionUnknown,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenditionKind {
    None,
    Color,
    RawData,
    ThemeCBCK,
    MultisizeImageSet,
    Unknown,
}

#[derive(Debug, Clone, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct RenditionColor {
    pub version: u32,
    pub color_space: ColorSpace,
    _component_count: u32,
    #[deku(count = "_component_count")]
    pub components: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct RenditionRawData {
    pub version: u32,
    _raw_data_length: u32,
    pub raw_data: PayloadBytes,
}

#[derive(Debug, Clone)]
pub struct RenditionThemeCBCK {
    pub version: u32,
    pub compression_type: CompressionType,
    _raw_data_length: u32,
    pub chunks: Vec<ThemeChunk>,
}

impl RenditionThemeCBCK {
    pub fn raw_datas(&self) -> impl Iterator<Item = &[u8]> {
        self.chunks.iter().map(|chunk| chunk.raw_data.as_slice())
    }
}

#[derive(Debug, Clone, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct RenditionMultisizeImageSet {
    pub version: u32,
    _count: u32,
    #[deku(count = "_count")]
    pub entries: Vec<MultisizeImageSetEntry>,
}

#[derive(Debug, Clone)]
pub struct RenditionUnknown {
    // pub tag: [u8; 4],
    pub version: u32,
    _raw_data_length: u32,
    pub raw_data: PayloadBytes,
}

#[derive(Debug, Clone, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct CBCK {
    pub idk: u32,
    pub tag: u32,
    pub a: u32,
    pub b: u32,
}

#[derive(Debug, Clone, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct MultisizeImageSetEntry {
    pub width: u32,
    pub height: u32,
    pub index: u16,
    pub idiom: Idiom,
}

#[derive(Debug, Clone, DekuRead)]
#[deku(endian = "little")]
pub struct ThemePartHeader {
    pub tag: [u8; 4],
    pub arg2: u32,
    pub arg3: u32,
    pub height: u32,
    pub raw_len: u32,
}

#[derive(Debug, Clone)]
pub struct ThemeChunk {
    pub part_header: Option<ThemePartHeader>,
    pub raw_data: PayloadBytes,
}

pub(crate) fn parse_rendition_payload(payload: PayloadBytes) -> BOMResult<Rendition> {
    let mut reader = Reader::new(BOMBlock::new(payload));
    let mut tag = [0_u8; 4];
    reader.read_bytes(4, &mut tag, Order::default())?;

    match &tag {
        b"RLOC" => {
            let data =
                RenditionColor::from_reader_with_ctx(&mut reader, deku::ctx::Endian::Little)?;
            Ok(Rendition::Color(data))
        }
        b"DWAR" => {
            let version = read_u32(&mut reader)?;
            let raw_data_length = read_u32(&mut reader)?;
            let raw_data = payload_slice(&mut reader, raw_data_length)?;
            Ok(Rendition::RawData(RenditionRawData {
                version,
                _raw_data_length: raw_data_length,
                raw_data,
            }))
        }
        b"MLEC" => {
            let version = read_u32(&mut reader)?;
            let compression_type =
                CompressionType::from_reader_with_ctx(&mut reader, deku::ctx::Endian::Little)?;
            let raw_data_length = read_u32(&mut reader)?;
            let chunks = read_theme_chunks(&mut reader, version, raw_data_length)?;
            Ok(Rendition::ThemeCBCK(RenditionThemeCBCK {
                version,
                compression_type,
                _raw_data_length: raw_data_length,
                chunks,
            }))
        }
        b"SISM" => {
            let data = RenditionMultisizeImageSet::from_reader_with_ctx(
                &mut reader,
                deku::ctx::Endian::Little,
            )?;
            Ok(Rendition::MultisizeImageSet(data))
        }
        _ => {
            let version = read_u32(&mut reader)?;
            let raw_data_length = read_u32(&mut reader)?;
            let raw_data = payload_slice(&mut reader, raw_data_length)?;
            Ok(Rendition::Unknown {
                tag,
                data: RenditionUnknown {
                    version,
                    _raw_data_length: raw_data_length,
                    raw_data,
                },
            })
        }
    }
}

fn read_u32(reader: &mut Reader<BOMBlock>) -> Result<u32, DekuError> {
    u32::from_reader_with_ctx(reader, deku::ctx::Endian::Little)
}

fn payload_slice(reader: &mut Reader<BOMBlock>, len: u32) -> BOMResult<PayloadBytes> {
    let len = len as usize;
    let slice = reader.as_mut().slice_at_current(len)?;
    reader.bits_read = reader.bits_read.saturating_add(len.saturating_mul(8));
    reader.leftover = None;
    Ok(slice)
}

fn read_theme_chunks(
    reader: &mut Reader<BOMBlock>,
    version: u32,
    raw_len: u32,
) -> BOMResult<Vec<ThemeChunk>> {
    match version {
        0 | 2 => {
            let raw_data = payload_slice(reader, raw_len)?;
            Ok(vec![ThemeChunk {
                part_header: None,
                raw_data,
            }])
        }
        1 | 3 => {
            let mut result = vec![];
            for _ in 0..raw_len {
                let part_header = ThemePartHeader::from_reader_with_ctx(reader, ())?;
                let raw_data = payload_slice(reader, part_header.raw_len)?;
                result.push(ThemeChunk {
                    part_header: Some(part_header),
                    raw_data,
                });
            }
            Ok(result)
        }
        _ => Err(BOMEror::ParseStruct(DekuError::Parse(
            format!("Not support version {}", version).into(),
        ))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, DekuRead)]
#[deku(id_type = "u32", endian = "little", ctx = "endian: deku::ctx::Endian")]
pub enum CompressionType {
    #[deku(id = 0)]
    Uncompressed,
    #[deku(id = 1)]
    Rle,
    #[deku(id = 2)]
    Zip,
    #[deku(id = 3)]
    Lzvn,
    #[deku(id = 4)]
    Lzfse,
    #[deku(id = 5)]
    JpegLzfse,
    #[deku(id = 6)]
    Blurred,
    #[deku(id = 7)]
    Astc,
    #[deku(id = 8)]
    PaletteImg,
    #[deku(id = 9)]
    HEVC, // ?
    #[deku(id = 10)]
    DeepmapLzfse,
    // Unknow,
    #[deku(id = 11)]
    Deepmap2,

    #[deku(id_pat = "_")]
    Unknown { tag: u32 },
}

#[derive(Debug, Clone, DekuRead)]
#[deku(id_type = "u16", endian = "little", ctx = "endian: deku::ctx::Endian")]
pub enum Idiom {
    #[deku(id = 0)]
    Universal,
    #[deku(id = 1)]
    Phone,
    #[deku(id = 2)]
    Pad,
    #[deku(id = 3)]
    TV,
    #[deku(id = 4)]
    Car,
    #[deku(id = 5)]
    Watch,
    #[deku(id = 6)]
    Marketing,

    #[deku(id_pat = "_")]
    Unknown { tag: u16 },
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct BitmapList {
    pub bitmap_count: u32, // usually 1?
    pub zero: u32,         // usually 0?
    pub rendition_length: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, DekuRead)]
#[deku(id_type = "u32", endian = "little", ctx = "endian: deku::ctx::Endian")]
pub enum LayoutType {
    #[deku(id = 6)]
    Gradient,
    #[deku(id = 7)]
    Effect,
    #[deku(id = 9)]
    Vector,
    #[deku(id = 10)]
    OnePartFixedSize,
    #[deku(id = 11)]
    OnePartTile,
    #[deku(id = 12)]
    OnePartScale,
    #[deku(id = 20)]
    ThreePartHorizontalTile,
    #[deku(id = 21)]
    ThreePartHorizontalScale,
    #[deku(id = 22)]
    ThreePartHorizontalUniform,
    #[deku(id = 23)]
    ThreePartVerticalTile,
    #[deku(id = 24)]
    ThreePartVerticalScale,
    #[deku(id = 25)]
    ThreePartVerticalUniform,
    #[deku(id = 30)]
    NinePartTile,
    #[deku(id = 31)]
    NinePartScale,
    #[deku(id = 32)]
    NinePartHorizontalUniformVerticalScale,
    #[deku(id = 33)]
    NinePartHorizontalScaleVerticalUniform,
    #[deku(id = 34)]
    NinePartEdgesOnly,
    #[deku(id = 40)]
    SixPart,
    #[deku(id = 50)]
    AnimationFilmstrip,

    #[deku(id = 1000)]
    Data,
    #[deku(id = 1001)]
    ExternalLink,
    #[deku(id = 1002)]
    LayerStack,
    #[deku(id = 1003)]
    InternalReference,
    #[deku(id = 1004)]
    PackedImage,
    #[deku(id = 1005)]
    NameList,
    #[deku(id = 1006)]
    UnknownAddObject,
    #[deku(id = 1007)]
    Texture,
    #[deku(id = 1008)]
    TextureImage,
    #[deku(id = 1009)]
    Color,
    #[deku(id = 1010)]
    MultisizeImage,
    #[deku(id = 1012)]
    LayerReference,
    #[deku(id = 1013)]
    ContentRendition,
    #[deku(id = 1014)]
    RecognitionObject,

    #[deku(id_pat = "_")]
    Unknown { tag: u32 },
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct Flags {
    #[deku(bits = 1)]
    pub is_header_flagged_fpo: bool,
    #[deku(bits = 1)]
    pub is_excluded_from_contrast_filter: bool,
    #[deku(bits = 1)]
    pub is_vector_based: bool,
    #[deku(bits = 1)]
    pub is_opaque: bool,
    #[deku(bits = 4)]
    pub bitmap_encoding: u8,
    #[deku(bits = 1)]
    pub opt_out_of_thinning: bool,
    #[deku(bits = 1)]
    pub is_flippable: bool,
    #[deku(bits = 1)]
    pub is_tintable: bool,
    #[deku(bits = 1)]
    pub preserved_vector_representation: bool,
    #[deku(bits = 20)]
    _reserved: u32,
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct Slice {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct Metric {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, DekuRead)]
#[deku(id_type = "u32", endian = "little", ctx = "endian: deku::ctx::Endian")]
pub enum RenditionType {
    #[deku(id = 1001)]
    Slices(RenditionTypeSlice),
    #[deku(id = 1003)]
    Metrics(RenditionTypeMetric),
    #[deku(id = 1004)]
    BlendModeAndOpacity(RenditionTypeBlendModeAndOpacity),
    #[deku(id = 1005)]
    UTI(RenditionTypeUTI),
    #[deku(id = 1006)]
    EXIFOrientation(RenditionTypeEXIFOrientation),
    #[deku(id = 1007)]
    BytesPerRow { length: u32, bytes_per_row: u32 },
    #[deku(id = 1010)]
    Reference(RenditionTypeReference),
    #[deku(id_pat = "_")]
    Unknown {
        tag: u32,
        data: RenditionTypeUnknown,
    },
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct RenditionTypeSlice {
    _length: u32,
    _count: u32,
    #[deku(count = "_count")]
    pub data: Vec<Slice>,
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct RenditionTypeMetric {
    _length: u32,
    _count: u32,
    pub top_right_inset: Metric,
    pub bottom_left_inset: Metric,
    pub image_size: Metric,
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct RenditionTypeBlendModeAndOpacity {
    _length: u32,
    pub blend_mode: u32,
    pub opacity: f32,
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct RenditionTypeUTI {
    _length: u32,
    pub string_length: u32,
    pub padding: u32,
    #[deku(count = "string_length")]
    pub string: Vec<u8>,
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct RenditionTypeEXIFOrientation {
    _length: u32,
    pub orientation: EXIFOrientationValue,
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct RenditionTypeBytesPerRow {
    _length: u32,
    pub bytes_per_row: u32,
}

#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct RenditionTypeReference {
    _length: u32,
    _magic: [u8; 4], // INLK
    _padding: u32,   // always 0?
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub layout: u16, // since rendition header says internal link
    _key_length: u32,
    #[deku(count = "_key_length")]
    pub keys: Vec<u8>, // rendition containing data
}
#[derive(Debug, DekuRead)]
#[deku(endian = "little", ctx = "endian: deku::ctx::Endian")]
pub struct RenditionTypeUnknown {
    _length: u32,
    #[deku(count = "_length")]
    pub data: Vec<u8>,
}

#[derive(Debug, DekuRead)]
#[deku(id_type = "u32", endian = "little", ctx = "endian: deku::ctx::Endian")]
pub enum EXIFOrientationValue {
    #[deku(id = 0)]
    None,
    #[deku(id = 1)]
    Normal,
    #[deku(id = 2)]
    Mirrored,
    #[deku(id = 3)]
    Rotated180,
    #[deku(id = 4)]
    Rotated180Mirrored,
    #[deku(id = 5)]
    Rotated90,
    #[deku(id = 6)]
    Rotated90Mirrored,
    #[deku(id = 7)]
    Rotated270,
    #[deku(id = 8)]
    Rotated270Mirrored,

    #[deku(id_pat = "_")]
    Unknown { tag: u32 },
}
