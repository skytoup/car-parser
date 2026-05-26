//! Deterministic export planning for `.car` assets.
//!
//! Planning is intentionally side-effect free: it resolves payload items,
//! computes output identities, sanitizes paths, coalesces canonical sources, and
//! assigns duplicate suffixes without creating directories or writing files.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crate::car::{CSIItem, Car, ReferenceRect};
use crate::model::Encoding;
use crate::model::rendition::{LayoutType, Rendition};
use crate::output::{self, OutputIdentity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Png,
    Jpeg,
    Webp,
    Heif,
    Pdf,
    Svg,
    Raw,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Heif => "heic",
            Self::Pdf => "pdf",
            Self::Svg => "svg",
            Self::Raw => "bin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportSkipReason {
    UnsupportedPayload,
    UnsafeFacetPath,
    UnsafeFileName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSkip {
    pub facet_name: String,
    pub rendition_name: String,
    pub reason: ExportSkipReason,
}

#[derive(Debug, Clone)]
pub struct ExportJob<'a> {
    pub logical_facet_name: String,
    pub path: PathBuf,
    pub logical_item: &'a CSIItem,
    pub payload_item: &'a CSIItem,
    pub output_identity: OutputIdentity<'a>,
    pub format: ExportFormat,
    pub crops: Vec<ReferenceRect>,
}

#[derive(Debug, Clone)]
pub struct ExportPlan<'a> {
    pub jobs: Vec<ExportJob<'a>>,
    pub skipped: Vec<ExportSkip>,
    pub canonical_coalesced: usize,
}

#[derive(Clone, Copy)]
struct OrderedItem<'a> {
    facet_name: &'a str,
    item: &'a CSIItem,
}

#[derive(Clone)]
struct PlannedItem<'a> {
    logical: OrderedItem<'a>,
    payload: OrderedItem<'a>,
    output_identity: OutputIdentity<'a>,
    crops: Vec<ReferenceRect>,
}

pub fn plan_export<'a>(car: &'a Car, output_root: impl AsRef<Path>) -> ExportPlan<'a> {
    let output_root = output_root.as_ref();
    let ordered_items = collect_ordered_items(car);
    let mut name_counts: HashMap<PathBuf, usize> = HashMap::new();
    let mut assigned_paths: HashSet<PathBuf> = HashSet::new();
    let mut seen_canonical_outputs: HashSet<Vec<u16>> = HashSet::new();
    let mut jobs = Vec::new();
    let mut skipped = Vec::new();
    let mut canonical_coalesced = 0usize;

    for logical in ordered_items.iter().copied() {
        let Some(planned) = resolve_planned_item(logical, &ordered_items, car) else {
            skipped.push(export_skip(logical, ExportSkipReason::UnsupportedPayload));
            continue;
        };

        if let Some(canonical_key) = planned.output_identity.canonical_identity_key()
            && !seen_canonical_outputs.insert(canonical_key)
        {
            canonical_coalesced += 1;
            continue;
        }

        let Some((path, format)) = build_planned_output_path(output_root, &planned) else {
            skipped.push(export_skip(logical, ExportSkipReason::UnsafeFileName));
            continue;
        };
        let Some(path) = assign_output_path(path, &mut name_counts, &mut assigned_paths) else {
            skipped.push(export_skip(logical, ExportSkipReason::UnsafeFileName));
            continue;
        };

        jobs.push(ExportJob {
            logical_facet_name: planned.logical.facet_name.to_string(),
            path,
            logical_item: planned.logical.item,
            payload_item: planned.payload.item,
            output_identity: planned.output_identity,
            format,
            crops: planned.crops,
        });
    }

    ExportPlan {
        jobs,
        skipped,
        canonical_coalesced,
    }
}

fn collect_ordered_items(car: &Car) -> Vec<OrderedItem<'_>> {
    let mut ordered_items = Vec::new();
    for (facet_name, facet) in car.named_facets() {
        let Some(items) = car.items_for_facet(facet) else {
            continue;
        };
        ordered_items.extend(items.iter().map(|item| OrderedItem { facet_name, item }));
    }
    ordered_items
}

fn resolve_planned_item<'a>(
    logical: OrderedItem<'a>,
    ordered_items: &[OrderedItem<'a>],
    car: &'a Car,
) -> Option<PlannedItem<'a>> {
    let (payload, crops) = resolve_payload_item(logical, ordered_items, car)?;
    let output_identity =
        car.output_identity_for_payload(logical.facet_name, logical.item, payload.item)?;

    Some(PlannedItem {
        logical,
        payload,
        output_identity,
        crops,
    })
}

fn resolve_payload_item<'a>(
    logical: OrderedItem<'a>,
    ordered_items: &[OrderedItem<'a>],
    car: &'a Car,
) -> Option<(OrderedItem<'a>, Vec<ReferenceRect>)> {
    if output::supported_output_identity(logical.item).is_some() {
        return Some((logical, Vec::new()));
    }

    if !matches!(logical.item.layout(), LayoutType::InternalReference) {
        return None;
    }

    if let Some(resolved) = car.resolve_internal_reference(logical.item) {
        let source = OrderedItem {
            facet_name: logical.facet_name,
            item: resolved.source,
        };
        return Some((source, resolved.crops));
    }

    let fallback = find_same_facet_payload(logical, ordered_items)?;
    Some((fallback, Vec::new()))
}

fn find_same_facet_payload<'a>(
    logical: OrderedItem<'a>,
    ordered_items: &[OrderedItem<'a>],
) -> Option<OrderedItem<'a>> {
    ordered_items.iter().copied().find(|candidate| {
        candidate.facet_name == logical.facet_name
            && !std::ptr::eq(candidate.item, logical.item)
            && candidate.item.name() == logical.item.name()
            && output::supported_output_identity(candidate.item).is_some()
    })
}

fn build_planned_output_path(
    output: &Path,
    planned: &PlannedItem<'_>,
) -> Option<(PathBuf, ExportFormat)> {
    let facet_path = sanitize_relative_path(planned.output_identity.facet_name())?;
    let (format, extension) =
        planned_extension(planned.output_identity.item(), planned.payload.item)?;
    let file_name = if matches!(
        planned.payload.item.header().rendition,
        Some(Rendition::Color(_)) | Some(Rendition::MultisizeImageSet(_))
    ) {
        planned_json_file_name(&planned.output_identity)?
    } else {
        output::suggested_file_name(&planned.output_identity, &extension)?
    };
    Some((output.join(facet_path).join(file_name), format))
}

pub fn sanitize_relative_path(path: &str) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    let mut has_component = false;

    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => {
                normalized.push(part);
                has_component = true;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    has_component.then_some(normalized)
}

fn planned_json_file_name(identity: &OutputIdentity<'_>) -> Option<String> {
    let base_name = output::sanitize_file_name(identity.item().name())?
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("asset"));

    let scale = output::rendition_scale(identity.item());
    let stem = if identity.include_scale_suffix() && scale > 1 {
        let suffix = format!("@{scale}x");
        if base_name.ends_with(&suffix) {
            base_name
        } else {
            format!("{base_name}{suffix}")
        }
    } else {
        base_name
    };

    Some(format!("{stem}.json"))
}

fn planned_extension(
    identity_item: &CSIItem,
    payload_item: &CSIItem,
) -> Option<(ExportFormat, String)> {
    match &payload_item.header().rendition {
        Some(Rendition::Color(_)) | Some(Rendition::MultisizeImageSet(_)) => Some((
            ExportFormat::Json,
            ExportFormat::Json.extension().to_string(),
        )),
        Some(Rendition::RawData(_)) => {
            let format = raw_format(payload_item.header().encoding);
            Some((format, preserved_file_extension(identity_item)?))
        }
        Some(Rendition::ThemeCBCK(_)) if should_save_raw_item(payload_item) => {
            let format = raw_format_for_item(payload_item);
            Some((
                format,
                output::default_raw_extension_for_item(payload_item).to_string(),
            ))
        }
        Some(Rendition::ThemeCBCK(_)) => {
            let (format, extension) = identity_item
                .name()
                .rsplit('.')
                .next()
                .and_then(image_extension)
                .unwrap_or((ExportFormat::Png, "png".to_string()));
            Some((format, extension))
        }
        _ => None,
    }
}

fn preserved_file_extension(item: &CSIItem) -> Option<String> {
    let file_name = output::sanitize_file_name(item.name())?;
    Some(
        file_name
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default(),
    )
}

fn image_extension(extension: &str) -> Option<(ExportFormat, String)> {
    let extension = extension.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some((ExportFormat::Png, extension)),
        "jpg" | "jpeg" => Some((ExportFormat::Jpeg, extension)),
        "webp" => Some((ExportFormat::Webp, extension)),
        _ => None,
    }
}

fn raw_format(encoding: Encoding) -> ExportFormat {
    match encoding {
        Encoding::HEIF => ExportFormat::Heif,
        Encoding::PDF => ExportFormat::Pdf,
        Encoding::SVG => ExportFormat::Svg,
        Encoding::JPEG => ExportFormat::Jpeg,
        Encoding::WEBP => ExportFormat::Webp,
        _ => ExportFormat::Raw,
    }
}

fn raw_format_for_item(item: &CSIItem) -> ExportFormat {
    if crate::image_conv::is_hevc_compressed(item) {
        ExportFormat::Heif
    } else {
        raw_format(item.header().encoding)
    }
}

pub fn should_save_raw(encoding: Encoding) -> bool {
    matches!(encoding, Encoding::HEIF | Encoding::PDF | Encoding::SVG)
}

pub fn should_save_raw_item(item: &CSIItem) -> bool {
    crate::image_conv::is_hevc_compressed(item) || should_save_raw(item.header().encoding)
}

fn assign_output_path(
    mut path: PathBuf,
    name_counts: &mut HashMap<PathBuf, usize>,
    assigned_paths: &mut HashSet<PathBuf>,
) -> Option<PathBuf> {
    if assigned_paths.contains(&path) {
        let base_path = path.clone();
        let stem = base_path.file_stem()?.to_string_lossy().to_string();
        let ext = base_path
            .extension()
            .map(|ext| ext.to_string_lossy().to_string())
            .unwrap_or_default();
        let parent = base_path.parent().map(Path::to_path_buf);
        let count = name_counts.entry(base_path).or_insert(0);

        loop {
            *count += 1;
            let new_name = if ext.is_empty() {
                format!("{stem}_{}", *count)
            } else {
                format!("{stem}_{}.{}", *count, ext)
            };
            let candidate = match &parent {
                Some(parent) => parent.join(new_name),
                None => PathBuf::from(new_name),
            };
            if !assigned_paths.contains(&candidate) {
                path = candidate;
                break;
            }
        }
    }

    assigned_paths.insert(path.clone());
    Some(path)
}

fn export_skip(logical: OrderedItem<'_>, reason: ExportSkipReason) -> ExportSkip {
    ExportSkip {
        facet_name: logical.facet_name.to_string(),
        rendition_name: logical.item.name().to_string(),
        reason,
    }
}
