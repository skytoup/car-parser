//! Stable serializable view objects for reporting `.car` contents.
//!
//! These types power `car-cli info` and are intended for callers that want
//! asset metadata without depending on raw CoreUI structs.
//!
//! Typical usage:
//!
//! ```rust
//! # use car::Car;
//! # fn demo(car: &Car) {
//! let document = car.document_info();
//! let renditions = car.rendition_infos();
//! # let _ = (document, renditions);
//! # }
//! ```

use serde::Serialize;

use crate::car::{CSIItem, Car};
use crate::model::rendition::{self, AttributeType, Idiom, LayoutType, Rendition};
use crate::{ColorModel, ColorSpace, Encoding};

#[derive(Debug, Clone, Serialize)]
pub struct DocumentInfo {
    #[serde(rename = "AssetStorageVersion")]
    pub asset_storage_version: u32,
    #[serde(rename = "CoreUIVersion")]
    pub core_ui_version: u32,
    #[serde(rename = "SchemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "Timestamp")]
    pub timestamp: u32,
    #[serde(rename = "MainVersionString")]
    pub main_version_string: String,
    #[serde(rename = "VersionString")]
    pub version_string: String,
    #[serde(rename = "UUID")]
    pub uuid: String,
    #[serde(rename = "ColorSpace")]
    pub color_space: &'static str,
    #[serde(rename = "KeySemantics")]
    pub key_semantics: u32,
    #[serde(rename = "RenditionCount")]
    pub rendition_count: u32,
    #[serde(rename = "AuthoringTool")]
    pub authoring_tool: String,
    #[serde(rename = "DeploymentPlatform")]
    pub deployment_platform: String,
    #[serde(rename = "DeploymentPlatformVersion")]
    pub deployment_platform_version: String,
    #[serde(rename = "ThinningArguments")]
    pub thinning_arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedPayloadInfo {
    #[serde(rename = "Encoding")]
    pub encoding: String,
    #[serde(rename = "ColorModel")]
    pub color_model: &'static str,
    #[serde(rename = "Opaque")]
    pub opaque: bool,
    #[serde(rename = "VectorBased")]
    pub vector_based: bool,
    #[serde(rename = "Flippable")]
    pub flippable: bool,
    #[serde(rename = "Tintable")]
    pub tintable: bool,
    #[serde(rename = "AssetType")]
    pub asset_type: &'static str,
    #[serde(
        rename = "Compression",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub compression: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum AttributeValue {
    String(String),
    Number(u16),
}

#[derive(Debug, Clone, Serialize)]
pub struct RenditionInfo {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "RenditionName")]
    pub rendition_name: String,
    #[serde(rename = "Width")]
    pub width: u32,
    #[serde(rename = "Height")]
    pub height: u32,
    #[serde(rename = "Scale")]
    pub scale: u32,
    #[serde(rename = "Layout")]
    pub layout: &'static str,
    #[serde(flatten)]
    pub payload: ResolvedPayloadInfo,
    #[serde(rename = "Idiom", skip_serializing_if = "Option::is_none", default)]
    pub idiom: Option<AttributeValue>,
    #[serde(
        rename = "AttributeScale",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub attribute_scale: Option<u16>,
    #[serde(rename = "Element", skip_serializing_if = "Option::is_none", default)]
    pub element: Option<u16>,
    #[serde(rename = "Part", skip_serializing_if = "Option::is_none", default)]
    pub part: Option<u16>,
    #[serde(rename = "State", skip_serializing_if = "Option::is_none", default)]
    pub state: Option<u16>,
    #[serde(rename = "Layer", skip_serializing_if = "Option::is_none", default)]
    pub layer: Option<u16>,
    #[serde(rename = "Value", skip_serializing_if = "Option::is_none", default)]
    pub value: Option<u16>,
    #[serde(rename = "Direction", skip_serializing_if = "Option::is_none", default)]
    pub direction: Option<u16>,
    #[serde(
        rename = "DisplayGamut",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub display_gamut: Option<u16>,
    #[serde(rename = "Subtype", skip_serializing_if = "Option::is_none", default)]
    pub subtype: Option<u16>,
    #[serde(
        rename = "HorizontalSizeClass",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub horizontal_size_class: Option<u16>,
    #[serde(
        rename = "VerticalSizeClass",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub vertical_size_class: Option<u16>,
    #[serde(
        rename = "DeploymentTarget",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub deployment_target: Option<u16>,
    #[serde(
        rename = "ThemeAppearance",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub theme_appearance: Option<u16>,
    #[serde(
        rename = "GlyphWeight",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub glyph_weight: Option<u16>,
    #[serde(rename = "GlyphSize", skip_serializing_if = "Option::is_none", default)]
    pub glyph_size: Option<u16>,
}

impl Car {
    pub fn document_info(&self) -> DocumentInfo {
        let header = self.header();
        let metadata = self.extended_metadata();

        DocumentInfo {
            asset_storage_version: header.storage_version,
            core_ui_version: header.coreui_version,
            schema_version: header.schema_version,
            timestamp: header.storage_timestamp,
            main_version_string: header.main_version_string.clone(),
            version_string: header.version_string.clone(),
            uuid: format_uuid(&header.uuid),
            color_space: color_space_str(&header.color_space),
            key_semantics: header.key_semantics,
            rendition_count: header.rendition_count,
            authoring_tool: metadata.authoring_tool.clone(),
            deployment_platform: metadata.deployment_platform.clone(),
            deployment_platform_version: metadata.deployment_platform_version.clone(),
            thinning_arguments: metadata.thinning_args.clone(),
        }
    }

    pub fn rendition_infos(&self) -> Vec<RenditionInfo> {
        let mut infos = Vec::new();

        for (facet_name, facet) in self.named_facets() {
            let Some(items) = self.items_for_facet(facet) else {
                continue;
            };
            for item in items {
                infos.push(self.build_rendition_info(facet_name, item));
            }
        }

        infos
    }

    fn build_rendition_info(&self, facet_name: &str, item: &CSIItem) -> RenditionInfo {
        let payload = if matches!(item.layout(), LayoutType::InternalReference) {
            self.resolve_internal_reference(item)
                .map(|resolved| payload_info(resolved.source))
                .unwrap_or_else(|| payload_info(item))
        } else {
            payload_info(item)
        };

        let mut info = RenditionInfo {
            name: facet_name.to_string(),
            rendition_name: item.name().to_string(),
            width: item.width(),
            height: item.height(),
            scale: item.scale(),
            layout: layout_type_str(&item.layout()),
            payload,
            idiom: None,
            attribute_scale: None,
            element: None,
            part: None,
            state: None,
            layer: None,
            value: None,
            direction: None,
            display_gamut: None,
            subtype: None,
            horizontal_size_class: None,
            vertical_size_class: None,
            deployment_target: None,
            theme_appearance: None,
            glyph_weight: None,
            glyph_size: None,
        };

        for attr in item.attributes() {
            match attr.name {
                AttributeType::Idiom => {
                    let idiom = idiom_attribute_value(attr.val);
                    info.idiom = Some(idiom);
                }
                AttributeType::Scale => info.attribute_scale = Some(attr.val),
                AttributeType::Element => info.element = Some(attr.val),
                AttributeType::Part => info.part = Some(attr.val),
                AttributeType::State => info.state = Some(attr.val),
                AttributeType::Layer => info.layer = Some(attr.val),
                AttributeType::Value => info.value = Some(attr.val),
                AttributeType::Direction => info.direction = Some(attr.val),
                AttributeType::DisplayGamut => info.display_gamut = Some(attr.val),
                AttributeType::Subtype => info.subtype = Some(attr.val),
                AttributeType::HorizontalSizeClass => {
                    info.horizontal_size_class = Some(attr.val);
                }
                AttributeType::VerticalSizeClass => {
                    info.vertical_size_class = Some(attr.val);
                }
                AttributeType::DeploymentTarget => info.deployment_target = Some(attr.val),
                AttributeType::ThemeAppearance => info.theme_appearance = Some(attr.val),
                AttributeType::GlyphWeight => info.glyph_weight = Some(attr.val),
                AttributeType::GlyphSize => info.glyph_size = Some(attr.val),
                _ => {}
            }
        }

        info
    }
}

fn payload_info(item: &CSIItem) -> ResolvedPayloadInfo {
    ResolvedPayloadInfo {
        encoding: encoding_str(&item.encoding()),
        color_model: color_model_str(&item.color_model()),
        opaque: item.header().flags.is_opaque,
        vector_based: item.header().flags.is_vector_based,
        flippable: item.header().flags.is_flippable,
        tintable: item.header().flags.is_tintable,
        asset_type: asset_type(item),
        compression: item
            .compression()
            .map(|compression| compression_type_str(&compression)),
    }
}

fn asset_type(item: &CSIItem) -> &'static str {
    match item.header().rendition.as_ref() {
        Some(Rendition::Color(_)) => "Color",
        Some(Rendition::RawData(_)) => "RawData",
        Some(Rendition::ThemeCBCK(_)) => "Image",
        Some(Rendition::MultisizeImageSet(_)) => "MultisizeImageSet",
        Some(Rendition::Unknown { .. }) => "Unknown",
        None => "None",
    }
}

fn idiom_attribute_value(idiom: u16) -> AttributeValue {
    let value = match idiom {
        0 => Some(Idiom::Universal),
        1 => Some(Idiom::Phone),
        2 => Some(Idiom::Pad),
        3 => Some(Idiom::TV),
        4 => Some(Idiom::Car),
        5 => Some(Idiom::Watch),
        6 => Some(Idiom::Marketing),
        _ => None,
    };

    match value {
        Some(idiom) => AttributeValue::String(idiom_str(&idiom).to_string()),
        None => AttributeValue::Number(idiom),
    }
}

fn encoding_str(enc: &Encoding) -> String {
    match enc {
        Encoding::None => "None".into(),
        Encoding::ARGB => "ARGB".into(),
        Encoding::Data => "Data".into(),
        Encoding::GRAY => "GRAY".into(),
        Encoding::JPEG => "JPEG".into(),
        Encoding::PDF => "PDF".into(),
        Encoding::WEBP => "WEBP".into(),
        Encoding::ARGB16 => "ARGB16".into(),
        Encoding::GA16 => "GA16".into(),
        Encoding::GA8 => "GA8".into(),
        Encoding::RGB5 => "RGB5".into(),
        Encoding::SVG => "SVG".into(),
        Encoding::HEIF => "HEIF".into(),
        Encoding::Unknown { tag } => format!("Unknown({tag:?})"),
    }
}

fn color_model_str(cm: &ColorModel) -> &'static str {
    match cm {
        ColorModel::None => "None",
        ColorModel::RGB => "RGB",
        ColorModel::Monochrome => "Monochrome",
        ColorModel::RGB0 => "RGB0",
        ColorModel::RGBP3 => "RGBP3",
        ColorModel::Unknown { .. } => "Unknown",
    }
}

fn color_space_str(cs: &ColorSpace) -> &'static str {
    match cs {
        ColorSpace::SRGB => "srgb",
        ColorSpace::GrayGamma2_2 => "gray-gamma-2.2",
        ColorSpace::DisplayP3 => "display-p3",
        ColorSpace::ExtendedRangeSRGB => "extended-srgb",
        ColorSpace::ExtendedLinearSRGB => "extended-linear-srgb",
        ColorSpace::ExtendedGray => "extended-gray",
        ColorSpace::SystemSRGB => "system-srgb",
        ColorSpace::Unknown { .. } => "unknown",
    }
}

fn layout_type_str(lt: &LayoutType) -> &'static str {
    match lt {
        LayoutType::Gradient => "Gradient",
        LayoutType::Effect => "Effect",
        LayoutType::Vector => "Vector",
        LayoutType::OnePartFixedSize => "OnePartFixedSize",
        LayoutType::OnePartTile => "OnePartTile",
        LayoutType::OnePartScale => "OnePartScale",
        LayoutType::ThreePartHorizontalTile => "ThreePartHorizontalTile",
        LayoutType::ThreePartHorizontalScale => "ThreePartHorizontalScale",
        LayoutType::ThreePartHorizontalUniform => "ThreePartHorizontalUniform",
        LayoutType::ThreePartVerticalTile => "ThreePartVerticalTile",
        LayoutType::ThreePartVerticalScale => "ThreePartVerticalScale",
        LayoutType::ThreePartVerticalUniform => "ThreePartVerticalUniform",
        LayoutType::NinePartTile => "NinePartTile",
        LayoutType::NinePartScale => "NinePartScale",
        LayoutType::NinePartHorizontalUniformVerticalScale => {
            "NinePartHorizontalUniformVerticalScale"
        }
        LayoutType::NinePartHorizontalScaleVerticalUniform => {
            "NinePartHorizontalScaleVerticalUniform"
        }
        LayoutType::NinePartEdgesOnly => "NinePartEdgesOnly",
        LayoutType::SixPart => "SixPart",
        LayoutType::AnimationFilmstrip => "AnimationFilmstrip",
        LayoutType::Data => "Data",
        LayoutType::ExternalLink => "ExternalLink",
        LayoutType::LayerStack => "LayerStack",
        LayoutType::InternalReference => "InternalReference",
        LayoutType::PackedImage => "PackedImage",
        LayoutType::NameList => "NameList",
        LayoutType::UnknownAddObject => "UnknownAddObject",
        LayoutType::Texture => "Texture",
        LayoutType::TextureImage => "TextureImage",
        LayoutType::Color => "Color",
        LayoutType::MultisizeImage => "MultisizeImage",
        LayoutType::LayerReference => "LayerReference",
        LayoutType::ContentRendition => "ContentRendition",
        LayoutType::RecognitionObject => "RecognitionObject",
        LayoutType::Unknown { .. } => "Unknown",
    }
}

fn idiom_str(idiom: &Idiom) -> &'static str {
    match idiom {
        Idiom::Universal => "universal",
        Idiom::Phone => "phone",
        Idiom::Pad => "pad",
        Idiom::TV => "tv",
        Idiom::Car => "car",
        Idiom::Watch => "watch",
        Idiom::Marketing => "marketing",
        Idiom::Unknown { .. } => "unknown",
    }
}

fn compression_type_str(ct: &rendition::CompressionType) -> &'static str {
    match ct {
        rendition::CompressionType::Uncompressed => "Uncompressed",
        rendition::CompressionType::Rle => "RLE",
        rendition::CompressionType::Zip => "Zip",
        rendition::CompressionType::Lzvn => "LZVN",
        rendition::CompressionType::Lzfse => "LZFSE",
        rendition::CompressionType::JpegLzfse => "JPEG+LZFSE",
        rendition::CompressionType::Blurred => "Blurred",
        rendition::CompressionType::Astc => "ASTC",
        rendition::CompressionType::PaletteImg => "PaletteImg",
        rendition::CompressionType::HEVC => "HEVC",
        rendition::CompressionType::DeepmapLzfse => "Deepmap+LZFSE",
        rendition::CompressionType::Deepmap2 => "Deepmap2",
        rendition::CompressionType::Unknown { .. } => "Unknown",
    }
}

fn format_uuid(uuid: &[u8; 16]) -> String {
    format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        uuid[0],
        uuid[1],
        uuid[2],
        uuid[3],
        uuid[4],
        uuid[5],
        uuid[6],
        uuid[7],
        uuid[8],
        uuid[9],
        uuid[10],
        uuid[11],
        uuid[12],
        uuid[13],
        uuid[14],
        uuid[15],
    )
}
