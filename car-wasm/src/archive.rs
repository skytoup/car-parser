use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use car::rendition::{CompressionType, LayoutType, Rendition, RenditionKind};
use car::{CSIItem, Car, ColorSpace, Encoding, ReferenceRect};

use crate::core;
use crate::error::{WasmError, WasmResult};
use crate::types::{
    DiagnosticIssueSummary, DiagnosticsSummary, DisplayPayload, DownloadPayload, DownloadStrategy,
    EntryInfo, EntryKind, EntryListItem, PreviewStrategy, SelectionReason, ThumbnailPayload,
};

const DEFAULT_THUMBNAIL_DIMENSION: u32 = 256;

#[derive(Debug, Clone)]
struct EntryRecord {
    key_values: Vec<u16>,
    info: EntryInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PreviewGroupKey {
    facet_name: String,
    rendition_name: String,
}

#[derive(Debug, Clone, Copy)]
struct PreviewCandidate {
    index: usize,
    scale: u32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ThumbnailCacheKey {
    entry_index: usize,
    max_dimension: u32,
}

#[derive(Debug, Clone)]
struct Classification {
    entry_kind: EntryKind,
    preview_strategy: PreviewStrategy,
    download_strategy: DownloadStrategy,
    selection_reason: SelectionReason,
    suggested_extension: String,
    mime_type: String,
    preserves_original_format: bool,
}

impl Classification {
    fn is_downloadable(&self) -> bool {
        !matches!(self.download_strategy, DownloadStrategy::None)
    }
}

struct ResolvedVisual<'a> {
    logical: &'a CSIItem,
    payload: &'a CSIItem,
    crops: Vec<ReferenceRect>,
}

pub struct ArchiveRuntime {
    car: Car,
    entries: Vec<EntryRecord>,
    entries_by_id: HashMap<String, usize>,
    thumbnail_cache: RefCell<HashMap<ThumbnailCacheKey, ThumbnailPayload>>,
}

impl ArchiveRuntime {
    pub fn load(bytes: Vec<u8>) -> WasmResult<Self> {
        let car = core::load_car_from_bytes(bytes)?;
        let mut entries = Vec::new();
        let mut entries_by_id = HashMap::new();

        for (facet_name, facet) in car.named_facets() {
            let Some(items) = car.items_for_facet(facet) else {
                continue;
            };

            for item in items {
                let Some(resolved) = resolve_visual(&car, item) else {
                    continue;
                };
                let Some(classification) = classify(&resolved) else {
                    continue;
                };
                let color_metadata = color_metadata(resolved.payload);

                let entry_index = entries.len();
                let id = format!("entry-{entry_index}");
                let downloadable = classification.is_downloadable();

                let suggested_file_name = if downloadable {
                    let output_identity = car
                        .output_identity_for_payload(facet_name, resolved.logical, resolved.payload)
                        .ok_or_else(|| {
                            WasmError::unsupported_encoding(format!(
                                "unable to determine output identity for `{}`",
                                item.name()
                            ))
                        })?;
                    car::suggested_file_name(&output_identity, &classification.suggested_extension)
                        .unwrap_or_else(|| fallback_asset_file_name(&classification))
                } else {
                    String::new()
                };

                entries_by_id.insert(id.clone(), entry_index);
                entries.push(EntryRecord {
                    key_values: item.key_values().to_vec(),
                    info: EntryInfo {
                        id,
                        preview_source_id: None,
                        facet_name: facet_name.to_string(),
                        rendition_name: item.name().to_string(),
                        width: item.width(),
                        height: item.height(),
                        scale: item.scale(),
                        logical_layout: layout_name(item.layout()).to_string(),
                        resolved_encoding: encoding_name(resolved.payload.encoding()).to_string(),
                        entry_kind: classification.entry_kind,
                        preview_strategy: classification.preview_strategy,
                        download_strategy: classification.download_strategy,
                        suggested_extension: classification.suggested_extension.clone(),
                        suggested_file_name,
                        mime_type: classification.mime_type.clone(),
                        preserves_original_format: classification.preserves_original_format,
                        selection_reason: classification.selection_reason,
                        downloadable,
                        color_space: color_metadata
                            .as_ref()
                            .map(|metadata| metadata.color_space.clone()),
                        color_components: color_metadata
                            .as_ref()
                            .map(|metadata| metadata.components.clone()),
                        css_color: color_metadata.map(|metadata| metadata.css_color),
                    },
                });
            }
        }

        assign_preview_source_ids(&mut entries);

        Ok(Self {
            car,
            entries,
            entries_by_id,
            thumbnail_cache: RefCell::new(HashMap::new()),
        })
    }

    pub fn document_info(&self) -> car::view::DocumentInfo {
        self.car.document_info()
    }

    pub fn diagnostics_summary(&self) -> DiagnosticsSummary {
        let report = self.car.diagnostics();
        let issue_samples = report
            .entries
            .iter()
            .flat_map(|entry| {
                entry.issues.iter().map(|issue| DiagnosticIssueSummary {
                    facet_name: entry.facet_name.clone(),
                    rendition_name: entry.rendition_name.clone(),
                    issue: format!("{issue:?}"),
                })
            })
            .take(50)
            .collect();

        DiagnosticsSummary {
            facets: report.totals.facets,
            entries: report.totals.entries,
            supported_outputs: report.totals.supported_outputs,
            unsupported_outputs: report.totals.unsupported_outputs,
            internal_references: report.totals.internal_references,
            unresolved_references: report.totals.unresolved_references,
            unknown_renditions: report.totals.unknown_renditions,
            unknown_tlvs: report.totals.unknown_tlvs,
            issue_samples,
        }
    }

    pub fn list_entries(&self) -> Vec<EntryInfo> {
        self.entries
            .iter()
            .map(|entry| entry.info.clone())
            .collect()
    }

    pub fn list_images(&self) -> Vec<EntryInfo> {
        self.entries
            .iter()
            .filter(|entry| is_legacy_image_entry(&entry.info))
            .map(|entry| entry.info.clone())
            .collect()
    }

    pub fn list_entry_summaries(&self) -> Vec<EntryListItem> {
        self.entries
            .iter()
            .map(|entry| entry.info.list_item())
            .collect()
    }

    pub fn list_image_summaries(&self) -> Vec<EntryListItem> {
        self.entries
            .iter()
            .filter(|entry| is_legacy_image_entry(&entry.info))
            .map(|entry| entry.info.list_item())
            .collect()
    }

    pub fn get_entry_info(&self, id: &str) -> WasmResult<EntryInfo> {
        Ok(self.entry(id)?.info.clone())
    }

    pub fn get_image_info(&self, id: &str) -> WasmResult<EntryInfo> {
        self.get_entry_info(id)
    }

    pub fn get_display_payload(&self, id: &str) -> WasmResult<DisplayPayload> {
        let entry = self.entry(id)?;
        let resolved = self.resolve_entry_item(entry)?;
        let classification = classify(&resolved).ok_or_else(|| {
            WasmError::unsupported_encoding(format!(
                "entry `{}` no longer resolves to a supported visual payload",
                entry.info.id
            ))
        })?;

        match classification.preview_strategy {
            PreviewStrategy::ImgBinary => {
                let bytes = core::resolved_source_bytes(&self.car, resolved.logical)?;
                Ok(DisplayPayload::ImgBinary {
                    mime_type: classification.mime_type.clone(),
                    suggested_extension: classification.suggested_extension.clone(),
                    suggested_file_name: entry.info.suggested_file_name.clone(),
                    preserves_original_format: classification.preserves_original_format,
                    selection_reason: classification.selection_reason,
                    bytes,
                })
            }
            PreviewStrategy::CanvasRgba => {
                let rgba = core::rgba_bytes_with_crops(resolved.payload, &resolved.crops)?;
                Ok(DisplayPayload::CanvasRgba {
                    width: resolved.logical.width(),
                    height: resolved.logical.height(),
                    suggested_extension: classification.suggested_extension.clone(),
                    suggested_file_name: entry.info.suggested_file_name.clone(),
                    preserves_original_format: classification.preserves_original_format,
                    selection_reason: classification.selection_reason,
                    rgba,
                })
            }
            PreviewStrategy::Document => {
                let bytes = core::resolved_source_bytes(&self.car, resolved.logical)?;
                Ok(DisplayPayload::Document {
                    mime_type: classification.mime_type.clone(),
                    suggested_extension: classification.suggested_extension.clone(),
                    suggested_file_name: entry.info.suggested_file_name.clone(),
                    preserves_original_format: classification.preserves_original_format,
                    selection_reason: classification.selection_reason,
                    bytes,
                })
            }
            PreviewStrategy::DownloadOnly => Ok(DisplayPayload::DownloadOnly {
                mime_type: classification.mime_type.clone(),
                suggested_extension: classification.suggested_extension.clone(),
                suggested_file_name: entry.info.suggested_file_name.clone(),
                preserves_original_format: classification.preserves_original_format,
                selection_reason: classification.selection_reason,
            }),
            PreviewStrategy::ColorSwatch => {
                let metadata = color_metadata(resolved.payload).ok_or_else(|| {
                    WasmError::decode_failed(format!(
                        "entry `{}` does not contain valid color metadata",
                        entry.info.id
                    ))
                })?;
                Ok(DisplayPayload::ColorSwatch {
                    color_space: metadata.color_space,
                    components: metadata.components,
                    css_color: metadata.css_color,
                })
            }
        }
    }

    pub fn get_download_payload(&self, id: &str) -> WasmResult<DownloadPayload> {
        let entry = self.entry(id)?;
        let resolved = self.resolve_entry_item(entry)?;
        let classification = classify(&resolved).ok_or_else(|| {
            WasmError::unsupported_encoding(format!(
                "entry `{}` no longer resolves to a supported visual payload",
                entry.info.id
            ))
        })?;

        let bytes = match classification.download_strategy {
            DownloadStrategy::Original => core::resolved_source_bytes(&self.car, resolved.logical)?,
            DownloadStrategy::Png => core::png_bytes_with_crops(resolved.payload, &resolved.crops)?,
            DownloadStrategy::None => {
                return Err(WasmError::entry_not_downloadable(&entry.info.id));
            }
        };

        Ok(DownloadPayload {
            bytes,
            mime_type: classification.mime_type.clone(),
            suggested_extension: classification.suggested_extension.clone(),
            suggested_file_name: entry.info.suggested_file_name.clone(),
            download_strategy: classification.download_strategy,
            preserves_original_format: classification.preserves_original_format,
            selection_reason: classification.selection_reason,
        })
    }

    pub fn get_thumbnail_payload(
        &self,
        id: &str,
        max_dimension: Option<u32>,
    ) -> WasmResult<ThumbnailPayload> {
        let (entry_index, entry) = self.entry_with_index(id)?;
        let max_dimension = normalize_thumbnail_dimension(max_dimension);
        let cache_key = ThumbnailCacheKey {
            entry_index,
            max_dimension,
        };

        if let Some(payload) = self.thumbnail_cache.borrow().get(&cache_key).cloned() {
            return Ok(payload);
        }

        let resolved = self.resolve_entry_item(entry)?;
        let classification = classify(&resolved).ok_or_else(|| {
            WasmError::unsupported_encoding(format!(
                "entry `{}` no longer resolves to a supported visual payload",
                entry.info.id
            ))
        })?;

        let payload = match classification.preview_strategy {
            PreviewStrategy::CanvasRgba => ThumbnailPayload::ImgBinary {
                mime_type: "image/png".to_string(),
                bytes: core::thumbnail_png_bytes_with_crops(
                    resolved.payload,
                    &resolved.crops,
                    max_dimension,
                )?,
            },
            PreviewStrategy::ImgBinary => match resolved.payload.encoding() {
                Encoding::JPEG | Encoding::WEBP
                    if resolved.logical.width().max(resolved.logical.height()) > max_dimension =>
                {
                    let source = core::resolved_source_bytes(&self.car, resolved.logical)?;
                    ThumbnailPayload::ImgBinary {
                        mime_type: "image/png".to_string(),
                        bytes: core::thumbnail_png_bytes_from_source(&source, max_dimension)?,
                    }
                }
                Encoding::JPEG | Encoding::WEBP | Encoding::SVG => ThumbnailPayload::ImgBinary {
                    mime_type: classification.mime_type.clone(),
                    bytes: core::resolved_source_bytes(&self.car, resolved.logical)?,
                },
                _ => {
                    return Err(WasmError::unsupported_encoding(format!(
                        "thumbnail preview does not support {:?} binary payloads",
                        resolved.payload.encoding()
                    )));
                }
            },
            PreviewStrategy::Document
            | PreviewStrategy::DownloadOnly
            | PreviewStrategy::ColorSwatch => ThumbnailPayload::DownloadOnly,
        };

        self.thumbnail_cache
            .borrow_mut()
            .insert(cache_key, payload.clone());
        Ok(payload)
    }

    fn entry(&self, id: &str) -> WasmResult<&EntryRecord> {
        let (_, entry) = self.entry_with_index(id)?;
        Ok(entry)
    }

    fn entry_with_index(&self, id: &str) -> WasmResult<(usize, &EntryRecord)> {
        let index = *self
            .entries_by_id
            .get(id)
            .ok_or_else(|| WasmError::entry_not_found(id))?;
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| WasmError::entry_not_found(id))?;
        Ok((index, entry))
    }

    fn resolve_entry_item<'a>(&'a self, entry: &'a EntryRecord) -> WasmResult<ResolvedVisual<'a>> {
        let logical = self
            .car
            .item_with_key_values(&entry.key_values)
            .ok_or_else(|| WasmError::entry_not_found(&entry.info.id))?;

        resolve_visual(&self.car, logical)
            .ok_or_else(|| WasmError::unresolved_reference(logical.name()))
    }
}

fn resolve_visual<'a>(car: &'a Car, logical: &'a CSIItem) -> Option<ResolvedVisual<'a>> {
    match logical.layout() {
        LayoutType::InternalReference => {
            let Ok(resolved) = car.try_resolve_internal_reference(logical) else {
                return None;
            };
            Some(ResolvedVisual {
                logical,
                payload: resolved.source,
                crops: resolved.crops,
            })
        }
        _ => Some(ResolvedVisual {
            logical,
            payload: logical,
            crops: Vec::new(),
        }),
    }
}

fn classify(resolved: &ResolvedVisual<'_>) -> Option<Classification> {
    let payload = resolved.payload;

    match payload.rendition_kind() {
        RenditionKind::Color => {
            return Some(Classification {
                entry_kind: EntryKind::Color,
                preview_strategy: PreviewStrategy::ColorSwatch,
                download_strategy: DownloadStrategy::None,
                selection_reason: SelectionReason::MetadataColor,
                suggested_extension: String::new(),
                mime_type: String::new(),
                preserves_original_format: true,
            });
        }
        RenditionKind::MultisizeImageSet | RenditionKind::None => return None,
        RenditionKind::Unknown => return None,
        RenditionKind::RawData | RenditionKind::ThemeCBCK => {}
    }

    if matches!(payload.compression(), Some(CompressionType::HEVC)) {
        return Some(Classification {
            entry_kind: EntryKind::Image,
            preview_strategy: PreviewStrategy::DownloadOnly,
            download_strategy: DownloadStrategy::Original,
            selection_reason: SelectionReason::DownloadOnlyOriginal,
            suggested_extension: "heic".to_string(),
            mime_type: "image/heic".to_string(),
            preserves_original_format: true,
        });
    }

    match payload.encoding() {
        Encoding::JPEG => Some(Classification {
            entry_kind: EntryKind::Image,
            preview_strategy: PreviewStrategy::ImgBinary,
            download_strategy: DownloadStrategy::Original,
            selection_reason: SelectionReason::OriginalBrowserBinary,
            suggested_extension: "jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            preserves_original_format: true,
        }),
        Encoding::WEBP => Some(Classification {
            entry_kind: EntryKind::Image,
            preview_strategy: PreviewStrategy::ImgBinary,
            download_strategy: DownloadStrategy::Original,
            selection_reason: SelectionReason::OriginalBrowserBinary,
            suggested_extension: "webp".to_string(),
            mime_type: "image/webp".to_string(),
            preserves_original_format: true,
        }),
        Encoding::SVG => Some(Classification {
            entry_kind: EntryKind::Image,
            preview_strategy: PreviewStrategy::ImgBinary,
            download_strategy: DownloadStrategy::Original,
            selection_reason: SelectionReason::OriginalBrowserBinary,
            suggested_extension: "svg".to_string(),
            mime_type: "image/svg+xml".to_string(),
            preserves_original_format: true,
        }),
        Encoding::PDF => Some(Classification {
            entry_kind: EntryKind::Document,
            preview_strategy: PreviewStrategy::DownloadOnly,
            download_strategy: DownloadStrategy::Original,
            selection_reason: SelectionReason::DownloadOnlyOriginal,
            suggested_extension: "pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            preserves_original_format: true,
        }),
        Encoding::HEIF => Some(Classification {
            entry_kind: EntryKind::Image,
            preview_strategy: PreviewStrategy::DownloadOnly,
            download_strategy: DownloadStrategy::Original,
            selection_reason: SelectionReason::DownloadOnlyOriginal,
            suggested_extension: "heic".to_string(),
            mime_type: "image/heic".to_string(),
            preserves_original_format: true,
        }),
        Encoding::ARGB
        | Encoding::ARGB16
        | Encoding::GRAY
        | Encoding::GA8
        | Encoding::GA16
        | Encoding::RGB5 => Some(Classification {
            entry_kind: EntryKind::Image,
            preview_strategy: PreviewStrategy::CanvasRgba,
            download_strategy: DownloadStrategy::Png,
            selection_reason: SelectionReason::DecodedRaster,
            suggested_extension: "png".to_string(),
            mime_type: "image/png".to_string(),
            preserves_original_format: false,
        }),
        Encoding::Data => {
            let suggested_extension = original_file_extension(resolved.logical);
            Some(Classification {
                entry_kind: EntryKind::RawData,
                preview_strategy: PreviewStrategy::DownloadOnly,
                download_strategy: DownloadStrategy::Original,
                selection_reason: SelectionReason::DownloadOnlyOriginal,
                mime_type: raw_data_mime_type(&suggested_extension).to_string(),
                suggested_extension,
                preserves_original_format: true,
            })
        }
        Encoding::None | Encoding::Unknown { .. } => None,
    }
}

fn fallback_asset_file_name(classification: &Classification) -> String {
    if classification.suggested_extension.is_empty() {
        "asset".to_string()
    } else {
        format!("asset.{}", classification.suggested_extension)
    }
}

fn original_file_extension(item: &CSIItem) -> String {
    Path::new(item.name())
        .file_name()
        .and_then(|name| Path::new(name).extension())
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

fn raw_data_mime_type(extension: &str) -> &'static str {
    match extension {
        "json" => "application/json",
        "lottie" => "application/zip",
        _ => "application/octet-stream",
    }
}

fn encoding_name(encoding: Encoding) -> &'static str {
    match encoding {
        Encoding::None => "none",
        Encoding::ARGB => "argb",
        Encoding::Data => "data",
        Encoding::GRAY => "gray",
        Encoding::JPEG => "jpeg",
        Encoding::PDF => "pdf",
        Encoding::WEBP => "webp",
        Encoding::ARGB16 => "argb16",
        Encoding::GA16 => "ga16",
        Encoding::GA8 => "ga8",
        Encoding::RGB5 => "rgb5",
        Encoding::SVG => "svg",
        Encoding::HEIF => "heif",
        Encoding::Unknown { .. } => "unknown",
    }
}

fn layout_name(layout: LayoutType) -> &'static str {
    match layout {
        LayoutType::Gradient => "gradient",
        LayoutType::Effect => "effect",
        LayoutType::Vector => "vector",
        LayoutType::OnePartFixedSize => "one-part-fixed-size",
        LayoutType::OnePartTile => "one-part-tile",
        LayoutType::OnePartScale => "one-part-scale",
        LayoutType::ThreePartHorizontalTile => "three-part-horizontal-tile",
        LayoutType::ThreePartHorizontalScale => "three-part-horizontal-scale",
        LayoutType::ThreePartHorizontalUniform => "three-part-horizontal-uniform",
        LayoutType::ThreePartVerticalTile => "three-part-vertical-tile",
        LayoutType::ThreePartVerticalScale => "three-part-vertical-scale",
        LayoutType::ThreePartVerticalUniform => "three-part-vertical-uniform",
        LayoutType::NinePartTile => "nine-part-tile",
        LayoutType::NinePartScale => "nine-part-scale",
        LayoutType::NinePartHorizontalUniformVerticalScale => {
            "nine-part-horizontal-uniform-vertical-scale"
        }
        LayoutType::NinePartHorizontalScaleVerticalUniform => {
            "nine-part-horizontal-scale-vertical-uniform"
        }
        LayoutType::NinePartEdgesOnly => "nine-part-edges-only",
        LayoutType::SixPart => "six-part",
        LayoutType::AnimationFilmstrip => "animation-filmstrip",
        LayoutType::Data => "data",
        LayoutType::ExternalLink => "external-link",
        LayoutType::LayerStack => "layer-stack",
        LayoutType::InternalReference => "internal-reference",
        LayoutType::PackedImage => "packed-image",
        LayoutType::NameList => "name-list",
        LayoutType::UnknownAddObject => "unknown-add-object",
        LayoutType::Texture => "texture",
        LayoutType::TextureImage => "texture-image",
        LayoutType::Color => "color",
        LayoutType::MultisizeImage => "multisize-image",
        LayoutType::LayerReference => "layer-reference",
        LayoutType::ContentRendition => "content-rendition",
        LayoutType::RecognitionObject => "recognition-object",
        LayoutType::Unknown { .. } => "unknown",
    }
}

fn normalize_thumbnail_dimension(max_dimension: Option<u32>) -> u32 {
    max_dimension.unwrap_or(DEFAULT_THUMBNAIL_DIMENSION).max(1)
}

fn assign_preview_source_ids(entries: &mut [EntryRecord]) {
    let mut candidates_by_group: HashMap<PreviewGroupKey, Vec<PreviewCandidate>> = HashMap::new();

    for (index, entry) in entries.iter().enumerate() {
        if !matches!(entry.info.preview_strategy, PreviewStrategy::DownloadOnly) {
            candidates_by_group
                .entry(preview_group_key(&entry.info))
                .or_default()
                .push(PreviewCandidate {
                    index,
                    scale: entry.info.scale,
                    width: entry.info.width,
                    height: entry.info.height,
                });
        }
    }

    let preview_source_ids: Vec<Option<String>> = entries
        .iter()
        .map(|entry| {
            if !matches!(entry.info.preview_strategy, PreviewStrategy::DownloadOnly) {
                return Some(entry.info.id.clone());
            }

            let candidates = candidates_by_group.get(&preview_group_key(&entry.info))?;
            let requested_scale = entry.info.scale;
            let best = candidates.iter().min_by_key(|candidate| {
                (
                    candidate_rank(requested_scale, **candidate),
                    candidate.scale,
                    preview_area(**candidate),
                    candidate.index,
                )
            })?;
            Some(entries[best.index].info.id.clone())
        })
        .collect();

    for (entry, preview_source_id) in entries.iter_mut().zip(preview_source_ids) {
        entry.info.preview_source_id = preview_source_id;
    }
}

fn preview_group_key(info: &EntryInfo) -> PreviewGroupKey {
    PreviewGroupKey {
        facet_name: info.facet_name.clone(),
        rendition_name: info.rendition_name.clone(),
    }
}

fn candidate_rank(requested_scale: u32, candidate: PreviewCandidate) -> u8 {
    if candidate.scale == requested_scale {
        0
    } else if candidate.scale == 1 {
        1
    } else {
        2
    }
}

fn preview_area(candidate: PreviewCandidate) -> u64 {
    (candidate.width as u64) * (candidate.height as u64)
}

fn is_legacy_image_entry(info: &EntryInfo) -> bool {
    !matches!(info.entry_kind, EntryKind::RawData | EntryKind::Color)
}

#[derive(Debug, Clone)]
struct ColorMetadata {
    color_space: String,
    components: Vec<f64>,
    css_color: String,
}

fn color_metadata(item: &CSIItem) -> Option<ColorMetadata> {
    let Rendition::Color(color) = item.header().rendition.as_ref()? else {
        return None;
    };

    let color_space = color_space_name(&color.color_space).to_string();
    let components = color.components.clone();
    let css_color = color_css_value(&color.color_space, &components);

    Some(ColorMetadata {
        color_space,
        components,
        css_color,
    })
}

fn color_space_name(color_space: &ColorSpace) -> &'static str {
    match color_space {
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

fn color_css_value(color_space: &ColorSpace, components: &[f64]) -> String {
    match color_space {
        ColorSpace::GrayGamma2_2 | ColorSpace::ExtendedGray => {
            let gray = clamp_color_component(components.first().copied().unwrap_or(0.0));
            let alpha = clamp_color_component(components.get(1).copied().unwrap_or(1.0));
            let gray_byte = color_byte(gray);
            format!("rgba({gray_byte}, {gray_byte}, {gray_byte}, {alpha:.4})")
        }
        ColorSpace::DisplayP3 => {
            let r = clamp_color_component(components.first().copied().unwrap_or(0.0));
            let g = clamp_color_component(components.get(1).copied().unwrap_or(0.0));
            let b = clamp_color_component(components.get(2).copied().unwrap_or(0.0));
            let alpha = clamp_color_component(components.get(3).copied().unwrap_or(1.0));
            format!("color(display-p3 {r:.4} {g:.4} {b:.4} / {alpha:.4})")
        }
        _ => {
            let r = color_byte(clamp_color_component(
                components.first().copied().unwrap_or(0.0),
            ));
            let g = color_byte(clamp_color_component(
                components.get(1).copied().unwrap_or(0.0),
            ));
            let b = color_byte(clamp_color_component(
                components.get(2).copied().unwrap_or(0.0),
            ));
            let alpha = clamp_color_component(components.get(3).copied().unwrap_or(1.0));
            format!("rgba({r}, {g}, {b}, {alpha:.4})")
        }
    }
}

fn clamp_color_component(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn color_byte(value: f64) -> u8 {
    (value * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smoke_fixture_archive() -> ArchiveRuntime {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../car-tests/data/Assets.car");
        let bytes = std::fs::read(&path).expect("read Assets.car");
        ArchiveRuntime::load(bytes).expect("load Assets.car")
    }

    fn full_fixture_archive() -> Option<ArchiveRuntime> {
        if !matches!(
            std::env::var("CAR_TEST_FULL").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
        ) {
            eprintln!("skipping full fixture test: set CAR_TEST_FULL=1 to enable Assets_iOS.car");
            return None;
        }

        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../car-tests/data/Assets_iOS.car");
        if !path.exists() {
            eprintln!("skipping full fixture test: missing {}", path.display());
            return None;
        }

        let bytes = std::fs::read(&path).expect("read Assets_iOS.car");
        Some(ArchiveRuntime::load(bytes).expect("load Assets_iOS.car"))
    }

    fn make_entry(
        id: &str,
        facet_name: &str,
        rendition_name: &str,
        scale: u32,
        size: (u32, u32),
        preview_strategy: PreviewStrategy,
        resolved_encoding: &str,
    ) -> EntryRecord {
        let (width, height) = size;
        EntryRecord {
            key_values: Vec::new(),
            info: EntryInfo {
                id: id.to_string(),
                preview_source_id: None,
                facet_name: facet_name.to_string(),
                rendition_name: rendition_name.to_string(),
                width,
                height,
                scale,
                logical_layout: "one-part-scale".to_string(),
                resolved_encoding: resolved_encoding.to_string(),
                entry_kind: EntryKind::Image,
                preview_strategy,
                download_strategy: match preview_strategy {
                    PreviewStrategy::DownloadOnly => DownloadStrategy::Original,
                    PreviewStrategy::ColorSwatch => DownloadStrategy::None,
                    _ => DownloadStrategy::Png,
                },
                suggested_extension: "png".to_string(),
                suggested_file_name: format!("{id}.png"),
                mime_type: "image/png".to_string(),
                preserves_original_format: !matches!(preview_strategy, PreviewStrategy::CanvasRgba),
                selection_reason: match preview_strategy {
                    PreviewStrategy::DownloadOnly => SelectionReason::DownloadOnlyOriginal,
                    PreviewStrategy::ImgBinary => SelectionReason::OriginalBrowserBinary,
                    PreviewStrategy::ColorSwatch => SelectionReason::MetadataColor,
                    PreviewStrategy::CanvasRgba | PreviewStrategy::Document => {
                        SelectionReason::DecodedRaster
                    }
                },
                downloadable: !matches!(preview_strategy, PreviewStrategy::ColorSwatch),
                color_space: None,
                color_components: None,
                css_color: None,
            },
        }
    }

    #[test]
    fn preview_source_pairing_prefers_same_scale_preview_entry() {
        let mut entries = vec![
            make_entry(
                "entry-raw",
                "Image/heic",
                "PlaygroundImage.heic",
                2,
                (0, 0),
                PreviewStrategy::DownloadOnly,
                "heif",
            ),
            make_entry(
                "entry-preview-1x",
                "Image/heic",
                "PlaygroundImage.heic",
                1,
                (256, 256),
                PreviewStrategy::CanvasRgba,
                "rgb5",
            ),
            make_entry(
                "entry-preview-2x",
                "Image/heic",
                "PlaygroundImage.heic",
                2,
                (512, 512),
                PreviewStrategy::CanvasRgba,
                "rgb5",
            ),
        ];

        assign_preview_source_ids(&mut entries);

        assert_eq!(
            entries[0].info.preview_source_id.as_deref(),
            Some("entry-preview-2x")
        );
        assert_eq!(
            entries[1].info.preview_source_id.as_deref(),
            Some("entry-preview-1x")
        );
        assert_eq!(
            entries[2].info.preview_source_id.as_deref(),
            Some("entry-preview-2x")
        );
    }

    #[test]
    fn preview_source_pairing_falls_back_to_scale_one_for_scale_zero_sources() {
        let mut entries = vec![
            make_entry(
                "entry-pdf",
                "Image/pdf",
                "PlaygroundImage.pdf",
                0,
                (0, 0),
                PreviewStrategy::DownloadOnly,
                "pdf",
            ),
            make_entry(
                "entry-preview-1x",
                "Image/pdf",
                "PlaygroundImage.pdf",
                1,
                (1536, 1536),
                PreviewStrategy::CanvasRgba,
                "argb",
            ),
            make_entry(
                "entry-preview-2x",
                "Image/pdf",
                "PlaygroundImage.pdf",
                2,
                (3072, 3072),
                PreviewStrategy::CanvasRgba,
                "argb",
            ),
        ];

        assign_preview_source_ids(&mut entries);

        assert_eq!(
            entries[0].info.preview_source_id.as_deref(),
            Some("entry-preview-1x")
        );
    }

    #[test]
    fn preview_source_pairing_keeps_unpaired_download_only_entries_empty() {
        let mut entries = vec![make_entry(
            "entry-pdf",
            "Image/pdf",
            "PlaygroundImage.pdf",
            0,
            (0, 0),
            PreviewStrategy::DownloadOnly,
            "pdf",
        )];

        assign_preview_source_ids(&mut entries);

        assert_eq!(entries[0].info.preview_source_id, None);
    }

    #[test]
    fn smoke_promptbar_hevc_entries_are_download_only_heic() {
        let archive = smoke_fixture_archive();
        let entries = archive.list_entries();
        let entry = entries
            .iter()
            .find(|entry| {
                entry.facet_name == "PromptBarBkg" && entry.rendition_name == "PromptBarBkg@2x.png"
            })
            .expect("PromptBarBkg@2x HEVC entry should be listed");

        assert_eq!(entry.resolved_encoding, "argb");
        assert_eq!(entry.preview_strategy, PreviewStrategy::DownloadOnly);
        assert_eq!(entry.download_strategy, DownloadStrategy::Original);
        assert_eq!(entry.suggested_extension, "heic");
        assert_eq!(entry.suggested_file_name, "PromptBarBkg@2x.heic");
        assert_eq!(entry.mime_type, "image/heic");

        let payload = archive
            .get_download_payload(&entry.id)
            .expect("HEVC entry should download original HEIC bytes");
        assert_eq!(&payload.bytes[4..8], b"ftyp");
        assert_eq!(&payload.bytes[8..12], b"heic");
    }

    #[test]
    fn assets_ios_lottie_raw_data_entries_keep_original_extensions() {
        let Some(archive) = full_fixture_archive() else {
            return;
        };

        let entries = archive.list_entries();
        let close = entries
            .iter()
            .find(|entry| entry.facet_name == "Lottie/TimelineReply/close")
            .expect("Lottie JSON entry should be listed");
        assert_eq!(close.entry_kind, EntryKind::RawData);
        assert_eq!(close.rendition_name, "close.json");
        assert_eq!(close.suggested_extension, "json");
        assert_eq!(close.suggested_file_name, "close.json");
        assert_eq!(close.mime_type, "application/json");

        let splash = entries
            .iter()
            .find(|entry| entry.facet_name == "Lottie/splash")
            .expect("Lottie dotlottie entry should be listed");
        assert_eq!(splash.suggested_extension, "lottie");
        assert_eq!(splash.suggested_file_name, "splash.lottie");

        let structured_image = entries
            .iter()
            .find(|entry| entry.facet_name == "Lottie/vip_one_month_card")
            .expect("extensionless Lottie RawData entry should be listed");
        assert_eq!(structured_image.suggested_extension, "");
        assert_eq!(structured_image.suggested_file_name, "CoreStructuredImage");

        assert!(
            archive
                .list_entry_summaries()
                .iter()
                .any(|entry| entry.facet_name == "Lottie/TimelineReply/close"),
            "full entry summaries should include Lottie raw data"
        );
        assert!(
            archive
                .list_image_summaries()
                .iter()
                .all(|entry| entry.facet_name != "Lottie/TimelineReply/close"),
            "legacy image summaries should remain image-only"
        );
    }
}
