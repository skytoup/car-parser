//! High-level asset model and typed variant queries.

use std::cmp::Ordering;

use thiserror::Error;

use crate::car::{CSIItem, Car, ReferenceRect};
use crate::metadata::TlvMetadata;
use crate::model::rendition::{AttributeType, CompressionType, LayoutType, Rendition};
use crate::model::{ColorModel, Encoding};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetId(u32);

impl AssetId {
    pub fn new(index: u32) -> Self {
        Self(index)
    }

    pub fn index(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Image,
    Color,
    Document,
    RawData,
    MultisizeImageSet,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadKind {
    Original,
    DecodedRaster,
    ColorMetadata,
    Dispatch,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedAttribute {
    pub name: AttributeType,
    pub value: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariantAttributes {
    pub scale: Option<u16>,
    pub idiom: Option<u16>,
    pub display_gamut: Option<u16>,
    pub appearance: Option<u16>,
    pub localization: Option<u16>,
    pub horizontal_size_class: Option<u16>,
    pub vertical_size_class: Option<u16>,
    pub all: Vec<TypedAttribute>,
    pub unknown: Vec<TypedAttribute>,
}

pub type AttributeSet = VariantAttributes;

impl VariantAttributes {
    pub fn raw(&self, name: AttributeType) -> Option<u16> {
        self.all
            .iter()
            .find(|attribute| attribute.name == name)
            .map(|attribute| attribute.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPayloadSummary {
    pub rendition_name: String,
    pub kind: AssetKind,
    pub payload_kind: PayloadKind,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub layout: LayoutType,
    pub encoding: Encoding,
    pub color_model: ColorModel,
    pub compression: Option<CompressionType>,
    pub key_values: Vec<u16>,
    pub effective_layout: u16,
    pub crops: Vec<ReferenceRect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetEntry {
    pub id: AssetId,
    pub facet_name: String,
    pub rendition_name: String,
    pub kind: AssetKind,
    pub payload_kind: PayloadKind,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub layout: LayoutType,
    pub encoding: Encoding,
    pub color_model: ColorModel,
    pub compression: Option<CompressionType>,
    pub attributes: VariantAttributes,
    pub metadata: TlvMetadata,
    pub variant_count: usize,
    pub resolved_payload: Option<ResolvedPayloadSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetVariant {
    pub asset_id: AssetId,
    pub facet_name: String,
    pub rendition_name: String,
    pub kind: AssetKind,
    pub payload_kind: PayloadKind,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub layout: LayoutType,
    pub encoding: Encoding,
    pub compression: Option<CompressionType>,
    pub attributes: VariantAttributes,
    pub key_values: Vec<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariantQuery {
    pub scale: Option<u16>,
    pub idiom: Option<u16>,
    pub display_gamut: Option<u16>,
    pub appearance: Option<u16>,
    pub localization: Option<u16>,
    pub horizontal_size_class: Option<u16>,
    pub vertical_size_class: Option<u16>,
}

impl VariantQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scale(mut self, scale: u16) -> Self {
        self.scale = Some(scale);
        self
    }

    pub fn idiom(mut self, idiom: u16) -> Self {
        self.idiom = Some(idiom);
        self
    }

    pub fn display_gamut(mut self, display_gamut: u16) -> Self {
        self.display_gamut = Some(display_gamut);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VariantMatchError {
    #[error("asset `{name}` was not found")]
    AssetNotFound { name: String },
    #[error("asset `{name}` has no variants")]
    NoVariants { name: String },
    #[error("asset `{name}` has no variant matching {query:?}")]
    NoMatch {
        name: String,
        query: VariantQuery,
        available: Vec<VariantAttributes>,
    },
}

impl CSIItem {
    pub fn variant_attributes(&self) -> VariantAttributes {
        let mut result = VariantAttributes::default();

        for attr in self.attributes() {
            let typed = TypedAttribute {
                name: attr.name,
                value: attr.val,
            };
            match attr.name {
                AttributeType::Scale => result.scale = Some(attr.val),
                AttributeType::Idiom => result.idiom = Some(attr.val),
                AttributeType::DisplayGamut => result.display_gamut = Some(attr.val),
                AttributeType::ThemeAppearance => result.appearance = Some(attr.val),
                AttributeType::Localization => result.localization = Some(attr.val),
                AttributeType::HorizontalSizeClass => result.horizontal_size_class = Some(attr.val),
                AttributeType::VerticalSizeClass => result.vertical_size_class = Some(attr.val),
                AttributeType::Unknown { .. } => result.unknown.push(typed.clone()),
                _ => {}
            }
            result.all.push(typed);
        }

        result
    }
}

struct LocatedAsset<'a> {
    id: AssetId,
    facet_name: &'a str,
    item: &'a CSIItem,
    facet_items: &'a [CSIItem],
    facet_start_id: u32,
}

impl Car {
    pub fn entries(&self) -> Vec<AssetEntry> {
        let mut entries = Vec::new();
        for (facet_name, facet) in self.named_facets() {
            let Some(items) = self.items_for_facet(facet) else {
                continue;
            };
            let variant_count = items.len();
            for item in items {
                entries.push(build_entry(
                    AssetId::new(entries.len() as u32),
                    facet_name,
                    item,
                    variant_count,
                    self,
                ));
            }
        }
        entries
    }

    pub fn entry(&self, id: AssetId) -> Option<AssetEntry> {
        let located = self.locate_asset(id)?;
        Some(build_entry(
            located.id,
            located.facet_name,
            located.item,
            located.facet_items.len(),
            self,
        ))
    }

    pub fn variants_for(&self, id: AssetId) -> Option<Vec<AssetVariant>> {
        let located = self.locate_asset(id)?;
        Some(
            located
                .facet_items
                .iter()
                .enumerate()
                .map(|(offset, item)| {
                    build_variant(
                        AssetId::new(located.facet_start_id.wrapping_add(offset as u32)),
                        located.facet_name,
                        item,
                    )
                })
                .collect(),
        )
    }

    pub fn variants_for_name(&self, name: &str) -> Option<Vec<AssetVariant>> {
        let mut variants = Vec::new();
        let mut next_id = 0u32;

        for (facet_name, facet) in self.named_facets() {
            let Some(items) = self.items_for_facet(facet) else {
                continue;
            };
            for item in items {
                let id = AssetId::new(next_id);
                next_id = next_id.wrapping_add(1);
                if facet_name == name {
                    variants.push(build_variant(id, facet_name, item));
                }
            }
        }

        (!variants.is_empty()).then_some(variants)
    }

    pub fn best_variant_for_name(
        &self,
        name: &str,
        query: &VariantQuery,
    ) -> Result<AssetVariant, VariantMatchError> {
        let variants =
            self.variants_for_name(name)
                .ok_or_else(|| VariantMatchError::AssetNotFound {
                    name: name.to_string(),
                })?;
        if variants.is_empty() {
            return Err(VariantMatchError::NoVariants {
                name: name.to_string(),
            });
        }

        if let Some(best) = variants
            .iter()
            .filter(|variant| matches_query(&variant.attributes, query))
            .min_by(|lhs, rhs| compare_variants(lhs, rhs, query))
            .cloned()
        {
            Ok(best)
        } else {
            Err(VariantMatchError::NoMatch {
                name: name.to_string(),
                query: query.clone(),
                available: variants
                    .into_iter()
                    .map(|variant| variant.attributes)
                    .collect(),
            })
        }
    }

    fn locate_asset(&self, id: AssetId) -> Option<LocatedAsset<'_>> {
        let mut next_id = 0u32;

        for (facet_name, facet) in self.named_facets() {
            let Some(items) = self.items_for_facet(facet) else {
                continue;
            };
            let facet_start_id = next_id;
            for item in items {
                let current_id = AssetId::new(next_id);
                next_id = next_id.wrapping_add(1);
                if current_id == id {
                    return Some(LocatedAsset {
                        id: current_id,
                        facet_name,
                        item,
                        facet_items: items,
                        facet_start_id,
                    });
                }
            }
        }

        None
    }
}

fn build_entry(
    id: AssetId,
    facet_name: &str,
    item: &CSIItem,
    variant_count: usize,
    car: &Car,
) -> AssetEntry {
    let resolved_payload = car
        .try_resolve_internal_reference(item)
        .ok()
        .map(|resolved| ResolvedPayloadSummary {
            rendition_name: resolved.source.name().to_string(),
            kind: asset_kind_for_item(resolved.source),
            payload_kind: payload_kind_for_item(resolved.source),
            width: resolved.source.width(),
            height: resolved.source.height(),
            scale: resolved.source.scale(),
            layout: resolved.source.layout(),
            encoding: resolved.source.encoding(),
            color_model: resolved.source.color_model(),
            compression: resolved.source.compression(),
            key_values: resolved.source.key_values().to_vec(),
            effective_layout: resolved.effective_layout,
            crops: resolved.crops,
        });

    AssetEntry {
        id,
        facet_name: facet_name.to_string(),
        rendition_name: item.name().to_string(),
        kind: asset_kind_for_item(item),
        payload_kind: payload_kind_for_item(item),
        width: item.width(),
        height: item.height(),
        scale: item.scale(),
        layout: item.layout(),
        encoding: item.encoding(),
        color_model: item.color_model(),
        compression: item.compression(),
        attributes: item.variant_attributes(),
        metadata: item.tlv_metadata(),
        variant_count,
        resolved_payload,
    }
}

fn build_variant(id: AssetId, facet_name: &str, item: &CSIItem) -> AssetVariant {
    AssetVariant {
        asset_id: id,
        facet_name: facet_name.to_string(),
        rendition_name: item.name().to_string(),
        kind: asset_kind_for_item(item),
        payload_kind: payload_kind_for_item(item),
        width: item.width(),
        height: item.height(),
        scale: item.scale(),
        layout: item.layout(),
        encoding: item.encoding(),
        compression: item.compression(),
        attributes: item.variant_attributes(),
        key_values: item.key_values().to_vec(),
    }
}

pub fn asset_kind_for_item(item: &CSIItem) -> AssetKind {
    match item.header().rendition.as_ref() {
        Some(Rendition::Color(_)) => AssetKind::Color,
        Some(Rendition::RawData(_)) => match item.encoding() {
            Encoding::PDF | Encoding::SVG => AssetKind::Document,
            _ => AssetKind::RawData,
        },
        Some(Rendition::ThemeCBCK(_)) => match item.encoding() {
            Encoding::PDF | Encoding::SVG => AssetKind::Document,
            _ => AssetKind::Image,
        },
        Some(Rendition::MultisizeImageSet(_)) => AssetKind::MultisizeImageSet,
        Some(Rendition::Unknown { .. }) | None => AssetKind::Unknown,
    }
}

pub fn payload_kind_for_item(item: &CSIItem) -> PayloadKind {
    if matches!(item.layout(), LayoutType::InternalReference) {
        return PayloadKind::Dispatch;
    }

    match item.header().rendition.as_ref() {
        Some(Rendition::Color(_)) => PayloadKind::ColorMetadata,
        Some(Rendition::RawData(_)) => PayloadKind::Original,
        Some(Rendition::ThemeCBCK(_)) => match item.encoding() {
            Encoding::ARGB
            | Encoding::ARGB16
            | Encoding::GRAY
            | Encoding::GA16
            | Encoding::GA8
            | Encoding::RGB5 => PayloadKind::DecodedRaster,
            Encoding::JPEG | Encoding::WEBP | Encoding::HEIF | Encoding::PDF | Encoding::SVG => {
                PayloadKind::Original
            }
            _ => PayloadKind::Unsupported,
        },
        Some(Rendition::MultisizeImageSet(_)) => PayloadKind::Original,
        Some(Rendition::Unknown { .. }) | None => PayloadKind::Unsupported,
    }
}

fn matches_query(attributes: &VariantAttributes, query: &VariantQuery) -> bool {
    optional_eq(attributes.scale, query.scale)
        && optional_eq(attributes.idiom, query.idiom)
        && optional_eq(attributes.display_gamut, query.display_gamut)
        && optional_eq(attributes.appearance, query.appearance)
        && optional_eq(attributes.localization, query.localization)
        && optional_eq(
            attributes.horizontal_size_class,
            query.horizontal_size_class,
        )
        && optional_eq(attributes.vertical_size_class, query.vertical_size_class)
}

fn optional_eq(actual: Option<u16>, expected: Option<u16>) -> bool {
    expected.is_none_or(|expected| actual == Some(expected))
}

fn compare_variants(lhs: &AssetVariant, rhs: &AssetVariant, query: &VariantQuery) -> Ordering {
    let lhs_scale = scale_rank(lhs, query.scale);
    let rhs_scale = scale_rank(rhs, query.scale);
    lhs_scale
        .cmp(&rhs_scale)
        .then_with(|| lhs.rendition_name.cmp(&rhs.rendition_name))
        .then_with(|| lhs.key_values.cmp(&rhs.key_values))
        .then_with(|| lhs.asset_id.cmp(&rhs.asset_id))
}

fn scale_rank(variant: &AssetVariant, requested: Option<u16>) -> (u8, u16) {
    let scale = variant.attributes.scale.unwrap_or(variant.scale as u16);
    match requested {
        Some(requested) if scale == requested => (0, scale),
        Some(requested) => (1, scale.abs_diff(requested)),
        None if scale == 1 => (0, scale),
        None => (1, scale),
    }
}
