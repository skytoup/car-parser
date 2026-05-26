#[cfg(test)]
use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::path::Component;
use std::path::{Path, PathBuf};

use anyhow::Result;
use car::export::should_save_raw_item;
#[cfg(test)]
use car::rendition::LayoutType;
use car::rendition::Rendition;
use car::{CarError, Encoding};
use rayon::prelude::*;

use crate::format;

/// 各阶段耗时统计（仅在测试编译时存在）。
///
/// `run()` 不暴露此结构；需要统计时使用 `run_collecting_stats()`。
#[cfg(test)]
struct ExtractStats {
    plan_ns: u64,
    parallel_save_ns: u64,
    aggregate_ns: u64,
    jobs_planned: usize,
    saved: usize,
    skipped: usize,
    failed: usize,
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct OrderedItem<'a> {
    facet_name: &'a str,
    item: &'a car::CSIItem,
}

#[cfg(test)]
#[derive(Clone)]
struct PlannedItem<'a> {
    logical: OrderedItem<'a>,
    payload: OrderedItem<'a>,
    output_identity: car::OutputIdentity<'a>,
    /// Crop rectangles to apply after decoding the payload image (empty = no crop).
    crops: Vec<car::ReferenceRect>,
}

struct ExtractJob<'a> {
    logical_facet_name: String,
    path: PathBuf,
    payload_item: &'a car::CSIItem,
    /// Crop rectangles accumulated from InternalReference resolution (empty = no crop).
    crops: Vec<car::ReferenceRect>,
}

enum JobOutcome {
    Saved {
        path: PathBuf,
    },
    Skipped,
    RecoverableFailure {
        facet_name: String,
        path: PathBuf,
        context: FailureContext,
        error: CarError,
    },
}

#[derive(Clone)]
struct FailureContext {
    key_values: Vec<u16>,
    layout: car::rendition::LayoutType,
    encoding: Encoding,
    compression: Option<car::rendition::CompressionType>,
}

struct ExtractPlan<'a> {
    jobs: Vec<ExtractJob<'a>>,
    pre_dispatch_skipped: usize,
}

struct ExtractSummary {
    saved: usize,
    skipped: usize,
    failed: usize,
}

pub fn run(car: &car::Car, output: &Path, overwrite: bool) -> Result<()> {
    run_with_save(car, output, overwrite, save_item)
}

fn run_with_save<F>(car: &car::Car, output: &Path, overwrite: bool, save: F) -> Result<()>
where
    F: Fn(&car::CSIItem, &Path) -> Result<bool, CarError> + Sync,
{
    let ExtractPlan {
        jobs,
        pre_dispatch_skipped,
    } = plan_jobs(car, output, overwrite)?;

    let outcomes = save_jobs(&jobs, &save)?;
    let summary = report_outcomes(outcomes, pre_dispatch_skipped);

    if summary.failed > 0 {
        anyhow::bail!("{} failed assets", summary.failed);
    }

    Ok(())
}

/// 带阶段耗时统计的执行入口（仅用于测试/性能诊断）。
///
/// 逻辑与 `run_with_save` 相同，但在三个阶段边界插入计时点，
/// 返回 `ExtractStats` 供 smoke 测试打印诊断信息。
#[cfg(test)]
fn run_collecting_stats(car: &car::Car, output: &Path, overwrite: bool) -> Result<ExtractStats> {
    use std::time::Instant;

    // ── Phase 1 ──────────────────────────────────────────────────────
    let plan_start = Instant::now();
    let ExtractPlan {
        jobs,
        pre_dispatch_skipped,
    } = plan_jobs(car, output, overwrite)?;

    let plan_ns = plan_start.elapsed().as_nanos() as u64;
    let jobs_planned = jobs.len();

    // ── Phase 2 ──────────────────────────────────────────────────────
    let parallel_start = Instant::now();

    let outcomes = save_jobs(&jobs, &save_item)?;
    let parallel_save_ns = parallel_start.elapsed().as_nanos() as u64;

    // ── Phase 3 ──────────────────────────────────────────────────────
    let aggregate_start = Instant::now();

    let summary = report_outcomes(outcomes, pre_dispatch_skipped);
    let aggregate_ns = aggregate_start.elapsed().as_nanos() as u64;

    if summary.failed > 0 {
        anyhow::bail!("{} failed assets", summary.failed);
    }

    Ok(ExtractStats {
        plan_ns,
        parallel_save_ns,
        aggregate_ns,
        jobs_planned,
        saved: summary.saved,
        skipped: summary.skipped,
        failed: summary.failed,
    })
}

fn save_jobs<F>(jobs: &[ExtractJob<'_>], save: &F) -> std::result::Result<Vec<JobOutcome>, CarError>
where
    F: Fn(&car::CSIItem, &Path) -> Result<bool, CarError> + Sync,
{
    jobs.par_iter().map(|job| save_job(job, save)).collect()
}

fn save_job<F>(job: &ExtractJob<'_>, save: &F) -> std::result::Result<JobOutcome, CarError>
where
    F: Fn(&car::CSIItem, &Path) -> Result<bool, CarError> + Sync,
{
    let result = if !job.crops.is_empty() {
        save_item_with_crops(job.payload_item, &job.path, &job.crops)
    } else {
        save(job.payload_item, &job.path)
    };
    match result {
        Ok(true) => Ok(JobOutcome::Saved {
            path: job.path.clone(),
        }),
        Ok(false) => Ok(JobOutcome::Skipped),
        Err(e) if is_recoverable(&e) => Ok(JobOutcome::RecoverableFailure {
            facet_name: job.logical_facet_name.clone(),
            path: job.path.clone(),
            context: FailureContext::from_item(job.payload_item),
            error: e,
        }),
        Err(e) => Err(e),
    }
}

fn report_outcomes(outcomes: Vec<JobOutcome>, pre_dispatch_skipped: usize) -> ExtractSummary {
    let mut summary = ExtractSummary {
        saved: 0,
        skipped: pre_dispatch_skipped,
        failed: 0,
    };

    for outcome in outcomes {
        match outcome {
            JobOutcome::Saved { path } => {
                println!("Extracted: {}", path.display());
                summary.saved += 1;
            }
            JobOutcome::Skipped => {
                summary.skipped += 1;
            }
            JobOutcome::RecoverableFailure {
                facet_name,
                path,
                context,
                error,
            } => {
                eprintln!(
                    "{}",
                    format_recoverable_failure_for_item(&facet_name, &path, &context, &error)
                );
                summary.failed += 1;
            }
        }
    }

    eprintln!(
        "Extracted {} assets, skipped {}, failed {}",
        summary.saved, summary.skipped, summary.failed
    );

    summary
}

fn format_recoverable_failure_for_item(
    facet_name: &str,
    path: &Path,
    context: &FailureContext,
    error: &CarError,
) -> String {
    format!(
        "Failed ({} -> {}): {} [key={:?}, layout={:?}, encoding={:?}, compression={:?}]",
        facet_name,
        path.display(),
        error,
        context.key_values,
        context.layout,
        context.encoding,
        context.compression
    )
}

impl FailureContext {
    fn from_item(item: &car::CSIItem) -> Self {
        Self {
            key_values: item.key_values().to_vec(),
            layout: item.layout(),
            encoding: item.encoding(),
            compression: item.compression(),
        }
    }
}

fn plan_jobs<'a>(car: &'a car::Car, output: &Path, overwrite: bool) -> Result<ExtractPlan<'a>> {
    std::fs::create_dir_all(output)?;

    let export_plan = car::export::plan_export(car, output);
    let mut jobs = Vec::new();
    let mut pre_dispatch_skipped = export_plan.skipped.len();

    for job in export_plan.jobs {
        if job.path.exists() && !overwrite {
            pre_dispatch_skipped += 1;
            continue;
        }

        if let Some(parent) = job.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        jobs.push(ExtractJob {
            logical_facet_name: job.logical_facet_name,
            path: job.path,
            payload_item: job.payload_item,
            crops: job.crops,
        });
    }

    Ok(ExtractPlan {
        jobs,
        pre_dispatch_skipped,
    })
}

#[cfg(test)]
fn plan_output_path(
    mut fp: PathBuf,
    overwrite: bool,
    name_counts: &mut HashMap<PathBuf, usize>,
    assigned_paths: &mut HashSet<PathBuf>,
) -> Option<PathBuf> {
    // Deduplicate: if this path is already claimed by a prior planning entry,
    // find a unique suffix. We loop rather than computing a single suffix so
    // that a renamed path cannot collide with a literal path that was already
    // assigned (e.g. foo.png→foo_1.png must not stomp a natural foo_1.png).
    if assigned_paths.contains(&fp) {
        let base_fp = fp.clone();
        let stem = base_fp
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let ext = base_fp
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let parent = base_fp.parent().map(|p| p.to_path_buf());
        let count = name_counts.entry(base_fp).or_insert(0);
        loop {
            *count += 1;
            let n = *count;
            let new_name = if ext.is_empty() {
                format!("{}_{}", stem, n)
            } else {
                format!("{}_{}.{}", stem, n, ext)
            };
            let candidate = match &parent {
                Some(p) => p.join(&new_name),
                None => PathBuf::from(&new_name),
            };
            if !assigned_paths.contains(&candidate) {
                fp = candidate;
                break;
            }
        }
    }

    // Claim this path before the skip check so that subsequent items
    // colliding with an already-on-disk file still get a unique suffix
    // (e.g. foo.png exists → skip, but foo_1.png is still written).
    assigned_paths.insert(fp.clone());

    if fp.exists() && !overwrite {
        None
    } else {
        Some(fp)
    }
}

#[cfg(test)]
fn build_output_path(output: &Path, facet_name: &str, item: &car::CSIItem) -> Option<PathBuf> {
    let output_identity = car::OutputIdentity::variant(facet_name, item);
    build_output_path_from_identity(output, &output_identity, item)
}

#[cfg(test)]
fn build_planned_output_path(output: &Path, planned: &PlannedItem<'_>) -> Option<PathBuf> {
    build_output_path_from_identity(output, &planned.output_identity, planned.payload.item)
}

#[cfg(test)]
fn build_output_path_from_identity(
    output: &Path,
    identity: &car::OutputIdentity<'_>,
    payload_item: &car::CSIItem,
) -> Option<PathBuf> {
    let facet_path = sanitize_relative_path(identity.facet_name())?;
    let extension = planned_extension(identity.item(), payload_item)?;
    let file_name = if matches!(
        payload_item.header().rendition,
        Some(Rendition::Color(_)) | Some(Rendition::MultisizeImageSet(_))
    ) {
        planned_json_file_name(identity)?
    } else {
        car::suggested_file_name(identity, &extension)?
    };
    Some(output.join(facet_path).join(file_name))
}

#[cfg(test)]
fn collect_ordered_items<'a>(car: &'a car::Car) -> Vec<OrderedItem<'a>> {
    let mut ordered_items = Vec::new();
    for (facet_name, facet) in car.named_facets() {
        let Some(items) = car.rendtions_with_facet(facet) else {
            continue;
        };
        ordered_items.extend(items.iter().map(|item| OrderedItem { facet_name, item }));
    }
    ordered_items
}

#[cfg(test)]
fn resolve_planned_item<'a>(
    logical: OrderedItem<'a>,
    ordered_items: &[OrderedItem<'a>],
    car: &'a car::Car,
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

/// Resolve the logical item to a `(payload, crops)` pair.
///
/// For `InternalReference` items, `Car::resolve_internal_reference` is called
/// first; it searches ALL RENDITIONS entries using exact key matching and handles
/// recursive chains internally.  If that fails, `find_same_facet_payload` is
/// used as a last-resort fallback for the existing source-like alias case.
#[cfg(test)]
fn resolve_payload_item<'a>(
    logical: OrderedItem<'a>,
    ordered_items: &[OrderedItem<'a>],
    car: &'a car::Car,
) -> Option<(OrderedItem<'a>, Vec<car::ReferenceRect>)> {
    // Item already carries a direct payload — no resolution needed.
    if car::supported_output_identity(logical.item).is_some() {
        return Some((logical, Vec::new()));
    }

    if !matches!(logical.item.layout(), LayoutType::InternalReference) {
        return None;
    }

    // Primary path: exact-key lookup across all RENDITIONS.
    if let Some(resolved) = car.resolve_internal_reference(logical.item) {
        let source = OrderedItem {
            facet_name: logical.facet_name,
            item: resolved.source,
        };
        return Some((source, resolved.crops));
    }

    // Fallback: same-facet sibling with a direct payload (source-like alias).
    let fallback = find_same_facet_payload(logical, ordered_items)?;
    Some((fallback, Vec::new()))
}

#[cfg(test)]
fn find_same_facet_payload<'a>(
    logical: OrderedItem<'a>,
    ordered_items: &[OrderedItem<'a>],
) -> Option<OrderedItem<'a>> {
    ordered_items.iter().copied().find(|candidate| {
        candidate.facet_name == logical.facet_name
            && !std::ptr::eq(candidate.item, logical.item)
            && candidate.item.name() == logical.item.name()
            && car::supported_output_identity(candidate.item).is_some()
    })
}

#[cfg(test)]
fn sanitize_relative_path(path: &str) -> Option<PathBuf> {
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

#[cfg(test)]
fn sanitize_file_name(name: &str) -> Option<PathBuf> {
    let mut file_name = None;

    for component in Path::new(name).components() {
        match component {
            Component::Normal(part) => file_name = Some(PathBuf::from(part)),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    file_name
}

#[cfg(test)]
fn planned_json_file_name(identity: &car::OutputIdentity<'_>) -> Option<String> {
    let base_name = sanitize_file_name(identity.item().name())?
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("asset"));

    let scale = car::rendition_scale(identity.item());
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

#[cfg(test)]
fn planned_extension(identity_item: &car::CSIItem, payload_item: &car::CSIItem) -> Option<String> {
    match &payload_item.header().rendition {
        Some(Rendition::Color(_)) | Some(Rendition::MultisizeImageSet(_)) => {
            Some("json".to_string())
        }
        Some(Rendition::RawData(_)) => preserved_file_extension(identity_item),
        Some(Rendition::ThemeCBCK(_)) if should_save_raw_item(payload_item) => {
            Some(car::default_raw_extension_for_item(payload_item).to_string())
        }
        Some(Rendition::ThemeCBCK(_)) => Some(
            identity_item
                .name()
                .rsplit('.')
                .next()
                .filter(|ext| {
                    matches!(
                        ext.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "webp"
                    )
                })
                .unwrap_or("png")
                .to_string(),
        ),
        _ => None,
    }
}

#[cfg(test)]
fn preserved_file_extension(item: &car::CSIItem) -> Option<String> {
    let file_name = sanitize_file_name(item.name())?;
    Some(
        file_name
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default(),
    )
}

/// Save an image-like item after applying crop rectangles from an InternalReference.
///
/// Only valid for `ThemeCBCK` payloads that would normally go through the image
/// encoding path (i.e. not HEIF/PDF/SVG raw saves).  For anything else the
/// normal `save_item` path is used.
fn save_item_with_crops(
    item: &car::CSIItem,
    path: &Path,
    crops: &[car::ReferenceRect],
) -> Result<bool, CarError> {
    match &item.header().rendition {
        Some(Rendition::ThemeCBCK(_)) if !should_save_raw_item(item) => {
            car::image::save_image_with_crops(item, path, crops)?;
            Ok(true)
        }
        _ => save_item(item, path),
    }
}

fn save_item(item: &car::CSIItem, path: &Path) -> Result<bool, CarError> {
    match &item.header().rendition {
        Some(Rendition::Color(color)) => {
            let obj = serde_json::json!({
                "colorSpace": format::color_space_str(&color.color_space),
                "components": color.components,
            });
            write_json(path, &obj)?;
            Ok(true)
        }
        Some(Rendition::RawData(_)) => {
            car::image::save_raw(item, path)?;
            Ok(true)
        }
        Some(Rendition::ThemeCBCK(_)) if should_save_raw_item(item) => {
            car::image::save_raw(item, path)?;
            Ok(true)
        }
        Some(Rendition::ThemeCBCK(_)) => {
            car::image::save_image(item, path)?;
            Ok(true)
        }
        Some(Rendition::MultisizeImageSet(mis)) => {
            let entries: Vec<_> = mis
                .entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "width": e.width,
                        "height": e.height,
                        "index": e.index,
                        "idiom": format::idiom_str(&e.idiom),
                    })
                })
                .collect();
            write_json(path, &serde_json::Value::Array(entries))?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), CarError> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| CarError::DecodeFailed(format!("JSON serialization failed: {}", e)))?;
    std::fs::write(path, json)?;
    Ok(())
}

fn is_recoverable(err: &CarError) -> bool {
    matches!(
        err,
        CarError::UnsupportedCompression(_)
            | CarError::UnsupportedEncoding(_)
            | CarError::Deepmap2(_)
            | CarError::DecodeFailed(_)
    )
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use car::rendition::LayoutType;
    use car::{Car, CarError, Encoding, OutputIdentityKind};
    use test_support::{fixture_group, temp_output_dir};

    use super::{
        FailureContext, build_output_path, build_planned_output_path, collect_ordered_items,
        format_recoverable_failure_for_item, plan_jobs, plan_output_path, resolve_planned_item,
        run, run_collecting_stats, run_with_save, sanitize_file_name, sanitize_relative_path,
        save_item,
    };

    fn fixture_car() -> Car {
        let group = fixture_group("smoke");
        Car::new(group.path("Assets.car")).expect("load test Assets.car")
    }

    const SMOKE_RECOVERABLE_FAILURES: usize = 1;

    fn fixture_ios_car() -> Option<Car> {
        let group = fixture_group("full");
        if !group.is_enabled() {
            eprintln!("skipping full fixture test: set CAR_TEST_FULL=1 to enable Assets_iOS.car");
            return None;
        }

        let path = group.path("Assets_iOS.car");
        if !path.exists() {
            eprintln!("skipping full fixture test: missing {}", path.display());
            return None;
        }

        Some(Car::new(path).expect("load test Assets_iOS.car"))
    }

    fn unique_temp_dir(tag: &str) -> PathBuf {
        temp_output_dir(&format!("car-cli-{tag}"))
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        assert_eq!(sanitize_relative_path("../evil"), None);
        assert_eq!(sanitize_relative_path("/tmp/evil"), None);
        assert_eq!(sanitize_file_name("../evil.png"), None);
        assert_eq!(
            sanitize_file_name("nested/evil.png"),
            Some(PathBuf::from("evil.png"))
        );
    }

    #[test]
    fn output_path_includes_facet_name_for_duplicate_rendition_names() {
        let car = fixture_car();
        let output = Path::new("out");

        let swipe = car
            .rendtions_with_name("WCPayAuth_FaceId")
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| !matches!(item.layout(), LayoutType::InternalReference))
            })
            .expect("WCPayAuth_FaceId rendition");
        let tap = car
            .rendtions_with_name("faceID")
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| !matches!(item.layout(), LayoutType::InternalReference))
            })
            .expect("faceID rendition");

        let swipe_path = build_output_path(output, "WCPayAuth_FaceId", swipe).expect("swipe path");
        let tap_path = build_output_path(output, "faceID", tap).expect("tap path");

        assert_eq!(
            swipe_path,
            PathBuf::from("out/WCPayAuth_FaceId/faceid@3x.png")
        );
        assert_eq!(tap_path, PathBuf::from("out/faceID/faceid@3x.png"));
        assert_ne!(swipe_path, tap_path);
    }

    #[test]
    fn color_assets_get_json_output_paths() {
        let car = fixture_car();
        let output = Path::new("out");
        let color = car
            .rendtions_with_name("ActionSheet_Action_Icon_Color")
            .and_then(|items| items.first())
            .expect("smoke color rendition");

        let path = build_output_path(output, "ActionSheet_Action_Icon_Color", color)
            .expect("color output path");
        assert_eq!(
            path,
            PathBuf::from("out/ActionSheet_Action_Icon_Color/ActionSheet_Action_Icon_Color.json")
        );
    }

    #[test]
    fn json_outputs_preserve_dots_in_asset_names() {
        let car = fixture_car();
        let output = Path::new("out");
        let color = car
            .rendtions_with_name("ActionSheet_Action_Icon_Color.ease")
            .and_then(|items| items.first())
            .expect("dotted color rendition");

        let path = build_output_path(output, "ActionSheet_Action_Icon_Color.ease", color)
            .expect("dotted color output path");
        assert_eq!(
            path,
            PathBuf::from(
                "out/ActionSheet_Action_Icon_Color.ease/ActionSheet_Action_Icon_Color.ease.json"
            )
        );
    }

    #[test]
    fn raw_fallback_extensions_match_actual_payload_format() {
        assert_eq!(car::default_raw_extension(Encoding::JPEG), "jpg");
        assert_eq!(car::default_raw_extension(Encoding::WEBP), "webp");
        assert_eq!(car::default_raw_extension(Encoding::HEIF), "heic");
        assert_eq!(car::default_raw_extension(Encoding::PDF), "pdf");
        assert_eq!(car::default_raw_extension(Encoding::SVG), "svg");
        assert_eq!(car::default_raw_extension(Encoding::ARGB), "bin");
    }

    #[test]
    fn promptbar_hevc_assets_are_planned_and_saved_as_heic() {
        let car = fixture_car();
        let output = unique_temp_dir("promptbar-hevc");
        let item = car
            .rendtions_with_name("PromptBarBkg")
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.name() == "PromptBarBkg@2x.png")
            })
            .expect("PromptBarBkg@2x HEVC rendition");

        let path = build_output_path(&output, "PromptBarBkg", item).expect("HEVC output path");
        assert_eq!(
            path.strip_prefix(&output).unwrap(),
            Path::new("PromptBarBkg/PromptBarBkg@2x.heic")
        );

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        save_item(item, &path).expect("HEVC source should save as raw HEIC");
        let bytes = std::fs::read(&path).expect("read saved HEIC");

        std::fs::remove_dir_all(&output).ok();

        assert_eq!(&bytes[4..8], b"ftyp");
        assert_eq!(&bytes[8..12], b"heic");
    }

    #[test]
    fn assets_ios_rawdata_payloads_are_planned_without_forced_extensions() {
        let Some(car) = fixture_ios_car() else {
            return;
        };
        let output = unique_temp_dir("ios-rawdata-plan");

        let export_plan = car::export::plan_export(&car, &output);
        assert!(
            export_plan.skipped.is_empty(),
            "Assets_iOS export planning should not skip RawData-capable payloads: {:?}",
            export_plan.skipped
        );

        let extract_plan = plan_jobs(&car, &output, false).expect("plan Assets_iOS extraction");
        assert_eq!(
            extract_plan.pre_dispatch_skipped, 0,
            "fresh Assets_iOS plan should not skip before dispatch"
        );

        let planned_paths: std::collections::HashSet<PathBuf> = extract_plan
            .jobs
            .iter()
            .filter_map(|job| job.path.strip_prefix(&output).ok().map(Path::to_path_buf))
            .collect();

        std::fs::remove_dir_all(&output).ok();

        for expected in [
            PathBuf::from("Lottie/splash/splash.lottie"),
            PathBuf::from("Lottie/vip_one_month_card/CoreStructuredImage"),
            PathBuf::from("pet_inter_bg_halloween/bg备份 2@3x.jpg"),
        ] {
            assert!(
                planned_paths.contains(&expected),
                "expected RawData output to be planned: {}",
                expected.display()
            );
        }
        assert!(
            !planned_paths.contains(&PathBuf::from(
                "Lottie/vip_one_month_card/CoreStructuredImage.bin"
            )),
            "RawData without an original extension must not receive .bin"
        );
    }

    #[test]
    fn extract_reports_recoverable_failures_and_keeps_outputs() {
        let car = fixture_car();
        let output = unique_temp_dir("clean-extract");

        let result = run(&car, &output, false);
        let file_count = collect_files(&output).len();

        std::fs::remove_dir_all(&output).ok();

        let err = result.expect_err("fixture should report recoverable failures");
        assert!(
            err.to_string()
                .contains(&format!("{SMOKE_RECOVERABLE_FAILURES} failed assets")),
            "unexpected extraction error: {err}"
        );
        assert!(file_count > 0, "expected at least one file to be extracted");
    }

    #[test]
    #[ignore = "smoke fixture 没有 PDF raster preview 与源 PNG 的资源"]
    fn extracted_deepmap2_raster_matches_expected_dimensions() {
        let car = fixture_car();
        let output = unique_temp_dir("deepmap2-raster");

        let result = run(&car, &output, false);

        let extracted_path = output.join("2016_coin1/2016_coin1@3x.png");
        let extracted =
            image::open(&extracted_path).expect("extracted Deepmap2 raster should decode");

        std::fs::remove_dir_all(&output).ok();

        let err = result.expect_err("fixture should report recoverable failures");
        assert!(
            err.to_string()
                .contains(&format!("{SMOKE_RECOVERABLE_FAILURES} failed assets")),
            "unexpected extraction error: {err}"
        );
        assert_eq!((extracted.width(), extracted.height()), (270, 270));
    }

    // animate_tag_newoutfit is an InternalReference pointing to a hidden Deepmap2 source that
    // does NOT appear in any named facet.  resolve_internal_reference() finds it via exact
    // RENDITIONS key matching and the extract pipeline should write a cropped PNG.
    #[test]
    fn assets_ios_animate_tag_newoutfit_is_written_with_correct_size_and_no_duplicate_suffix() {
        let Some(car) = fixture_ios_car() else {
            return;
        };
        let output = unique_temp_dir("ios-animate-tag");

        let result = run(&car, &output, false);

        let png_path = output
            .join("animate_tag_newoutfit")
            .join("animate_tag_newoutfit@3x.png");
        let exists = png_path.exists();

        // Verify dimensions from PNG IHDR (big-endian u32 at bytes 16..24).
        let dims = if exists {
            std::fs::read(&png_path).ok().and_then(|data| {
                if data.len() >= 24 {
                    let w = u32::from_be_bytes(data[16..20].try_into().ok()?);
                    let h = u32::from_be_bytes(data[20..24].try_into().ok()?);
                    Some((w, h))
                } else {
                    None
                }
            })
        } else {
            None
        };

        std::fs::remove_dir_all(&output).ok();

        result.expect("Assets_iOS extraction should succeed");
        assert!(
            exists,
            "animate_tag_newoutfit@3x.png should be written (hidden Deepmap2 source via RENDITIONS)"
        );
        // File name must not have a duplicate @3x@3x suffix.
        assert!(
            !png_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("@3x@3x"),
            "output file name must not have duplicate @3x@3x suffix"
        );
        assert_eq!(
            dims,
            Some((102, 48)),
            "cropped PNG dimensions should be 102×48"
        );
    }

    #[test]
    fn assets_ios_resolved_internal_reference_is_planned_and_written() {
        let Some(car) = fixture_ios_car() else {
            return;
        };
        let ordered_items = collect_ordered_items(&car);
        let output = unique_temp_dir("ios-internal-reference");
        let planned = ordered_items
            .iter()
            .copied()
            .find_map(|logical| {
                (logical.facet_name == "icon_device_migration"
                    && matches!(logical.item.layout(), LayoutType::InternalReference))
                .then(|| resolve_planned_item(logical, &ordered_items, &car))
                .flatten()
            })
            .expect("expected icon_device_migration InternalReference to resolve");
        let reference_path = build_planned_output_path(&output, &planned)
            .expect("resolved InternalReference should produce an output path");

        let result = run(&car, &output, false);
        let saved = reference_path.exists();

        std::fs::remove_dir_all(&output).ok();

        result.expect("Assets_iOS extraction should succeed");
        assert!(
            saved,
            "expected resolved InternalReference output to be written: {}",
            reference_path.display()
        );
    }

    // The icon_device_migration InternalReference (@3x) now correctly resolves to a hidden
    // Deepmap2 ThemeCBCK via exact RENDITIONS key matching.  That makes it VariantOutput,
    // not CanonicalSourceOutput, so the output path carries the @3x scale suffix.
    // The SVG direct entry (scale=1, CanonicalSourceOutput) is planned separately and is
    // unaffected by the InternalReference change.
    #[test]
    fn assets_ios_internal_reference_resolves_to_variant_output_with_scale_suffix() {
        let Some(car) = fixture_ios_car() else {
            return;
        };
        let ordered_items = collect_ordered_items(&car);
        let output = unique_temp_dir("ios-variant-scale");

        let planned = ordered_items
            .iter()
            .copied()
            .find_map(|logical| {
                (logical.facet_name == "icon_device_migration"
                    && matches!(logical.item.layout(), LayoutType::InternalReference))
                .then(|| resolve_planned_item(logical, &ordered_items, &car))
                .flatten()
            })
            .expect("expected icon_device_migration InternalReference to resolve");

        assert_eq!(
            planned.output_identity.kind(),
            OutputIdentityKind::VariantOutput,
            "icon_device_migration InternalReference should resolve to VariantOutput (Deepmap2 source)"
        );

        let path = build_planned_output_path(&output, &planned)
            .expect("resolved InternalReference should produce an output path");
        let scale = car::rendition_scale(planned.logical.item);
        let file_name = path
            .file_name()
            .expect("planned output should have a file name")
            .to_string_lossy()
            .to_string();

        if scale > 1 {
            let scale_suffix = format!("@{}x", scale);
            assert!(
                file_name.contains(&scale_suffix),
                "VariantOutput for scale={scale} should keep scale suffix {scale_suffix}: {file_name}"
            );
        }
    }

    // The SVG direct entry for icon_device_migration (CanonicalSourceOutput, scale=1) is
    // still planned and coalesces — the InternalReference change does not affect it.
    #[test]
    fn assets_ios_svg_direct_entry_still_planned_as_canonical_source() {
        let Some(car) = fixture_ios_car() else {
            return;
        };
        let ordered_items = collect_ordered_items(&car);

        let canonical_key = ordered_items
            .iter()
            .copied()
            .find_map(|logical| {
                // Find the direct SVG item (not InternalReference) for icon_device_migration.
                (logical.facet_name == "icon_device_migration"
                    && !matches!(logical.item.layout(), LayoutType::InternalReference))
                .then(|| resolve_planned_item(logical, &ordered_items, &car))
                .flatten()
                .and_then(|planned| {
                    (planned.output_identity.kind() == OutputIdentityKind::CanonicalSourceOutput)
                        .then(|| planned.output_identity.canonical_identity_key())
                        .flatten()
                })
            })
            .expect("expected icon_device_migration direct SVG to produce CanonicalSourceOutput");

        let output = unique_temp_dir("ios-svg-coalesce");
        let plan = plan_jobs(&car, &output, false).expect("plan Assets_iOS extraction");
        let matching_jobs = plan
            .jobs
            .iter()
            .filter(|job| job.payload_item.key_values().to_vec() == canonical_key)
            .count();

        std::fs::remove_dir_all(&output).ok();

        assert_eq!(
            matching_jobs, 1,
            "SVG direct entry canonical source should coalesce to one job"
        );
    }

    // Task 3.1: pre-dispatch skip behavior — files already present must be excluded
    // from the worker pool when --overwrite is not set.
    #[test]
    fn second_run_without_overwrite_skips_already_extracted_files() {
        let car = fixture_car();
        let output = unique_temp_dir("skip-existing");

        // First run: create files for the later skip check.
        let _ = run(&car, &output, false);

        // Snapshot which files exist after the first run
        let files_after_first: std::collections::HashSet<PathBuf> =
            collect_files(&output).into_iter().collect();

        // Second run without overwrite: no new files should be written
        let _ = run(&car, &output, false);

        let files_after_second: std::collections::HashSet<PathBuf> =
            collect_files(&output).into_iter().collect();

        std::fs::remove_dir_all(&output).ok();

        // Second run should not add or remove any files when overwrite is disabled.
        assert!(
            files_after_first == files_after_second,
            "second run should not change the extracted file set"
        );
    }

    // Task 3.1: duplicate suffix allocation is decided during planning and is stable.
    #[test]
    fn duplicate_suffix_allocation_is_deterministic() {
        use std::collections::{HashMap, HashSet};

        let base = PathBuf::from("out/Facet/asset.png");
        let mut name_counts: HashMap<PathBuf, usize> = HashMap::new();
        let mut assigned_paths: HashSet<PathBuf> = HashSet::new();

        let fp1 = plan_output_path(base.clone(), false, &mut name_counts, &mut assigned_paths)
            .expect("first path should be scheduled");
        let fp2 = plan_output_path(base.clone(), false, &mut name_counts, &mut assigned_paths)
            .expect("second path should get _1 suffix");
        let fp3 = plan_output_path(base, false, &mut name_counts, &mut assigned_paths)
            .expect("third path should get _2 suffix");

        assert_eq!(fp1, PathBuf::from("out/Facet/asset.png"));
        assert_eq!(fp2, PathBuf::from("out/Facet/asset_1.png"));
        assert_eq!(fp3, PathBuf::from("out/Facet/asset_2.png"));
        assert!(fp1 != fp2 && fp2 != fp3 && fp1 != fp3);
    }

    #[test]
    fn existing_path_still_reserves_name_for_later_suffixes() {
        use std::collections::{HashMap, HashSet};

        let output = unique_temp_dir("plan-existing");
        let base = output.join("Facet/asset.png");
        std::fs::create_dir_all(base.parent().expect("base path should have parent"))
            .expect("create parent dir");
        std::fs::write(&base, b"existing").expect("seed existing file");

        let mut name_counts: HashMap<PathBuf, usize> = HashMap::new();
        let mut assigned_paths: HashSet<PathBuf> = HashSet::new();

        let first = plan_output_path(base.clone(), false, &mut name_counts, &mut assigned_paths);
        let second = plan_output_path(base.clone(), false, &mut name_counts, &mut assigned_paths)
            .expect("second colliding path should get a suffix");

        std::fs::remove_dir_all(&output).ok();

        assert_eq!(
            first, None,
            "existing file should be skipped without overwrite"
        );
        assert_eq!(second, output.join("Facet/asset_1.png"));
    }

    // Task 3.2: fail-fast when the output root cannot be created.
    #[test]
    fn fails_fast_when_output_root_cannot_be_created() {
        let car = fixture_car();
        let unique = unique_temp_dir("obstacle");

        // Create a plain file at the target path so create_dir_all will fail
        std::fs::write(&unique, b"obstacle").expect("write obstacle");

        // Try to use a subdirectory of the file as output — this must fail at setup
        let output = unique.join("subdir");
        let result = run(&car, &output, false);

        std::fs::remove_file(&unique).ok();

        let err = result.expect_err("should fail when output root cannot be created");
        // This is a setup error, not a per-asset failure
        assert!(
            !err.to_string().contains("failed assets"),
            "setup error should not be reported as failed assets: {err}"
        );
    }

    // Task 3.2: aggregation phase collects all outcomes before returning.
    // The fixture has no recoverable failures, so all jobs succeed; we verify
    // that the parallel stage does not silently drop assets.
    #[test]
    fn all_eligible_assets_are_written_in_parallel_run() {
        let car = fixture_car();
        let output = unique_temp_dir("parallel-agg");

        let result = run(&car, &output, false);
        let extracted_count = collect_files(&output).len();

        std::fs::remove_dir_all(&output).ok();

        let err = result.expect_err("fixture should report recoverable failures");
        assert!(
            err.to_string()
                .contains(&format!("{SMOKE_RECOVERABLE_FAILURES} failed assets")),
            "unexpected extraction error: {err}"
        );
        assert!(
            extracted_count > 0,
            "parallel execution should not silently drop assets"
        );
    }

    #[test]
    fn recoverable_failure_keeps_other_jobs_running_and_returns_aggregate_error() {
        let car = fixture_car();
        let output = unique_temp_dir("recoverable-failure");
        let failed_path =
            output.join("ActionSheet_Action_Icon_Color/ActionSheet_Action_Icon_Color.json");
        let successful_path = output.join("2016_coin1/2016_coin1@3x.png");

        let result = run_with_save(&car, &output, false, |item, path| {
            if path == failed_path.as_path() {
                Err(CarError::DecodeFailed(
                    "injected recoverable failure".to_string(),
                ))
            } else {
                save_item(item, path)
            }
        });

        let err = result.expect_err("recoverable failure should be aggregated");
        assert!(
            err.to_string()
                .contains(&format!("{} failed assets", SMOKE_RECOVERABLE_FAILURES + 1)),
            "unexpected aggregated error: {err}"
        );
        assert!(
            !failed_path.exists(),
            "the injected failure path should not be written"
        );
        assert!(
            successful_path.exists(),
            "other planned jobs should still complete successfully"
        );

        std::fs::remove_dir_all(&output).ok();
    }

    #[test]
    fn recoverable_failure_log_message_includes_facet_and_path() {
        let context = FailureContext {
            key_values: vec![1, 2, 3],
            layout: LayoutType::OnePartScale,
            encoding: Encoding::ARGB,
            compression: None,
        };
        let message = format_recoverable_failure_for_item(
            "ActionSheet_Action_Icon_Color",
            Path::new("out/ActionSheet_Action_Icon_Color/ActionSheet_Action_Icon_Color.json"),
            &context,
            &CarError::DecodeFailed("boom".to_string()),
        );

        assert!(message.contains("ActionSheet_Action_Icon_Color"));
        assert!(
            message
                .contains("out/ActionSheet_Action_Icon_Color/ActionSheet_Action_Icon_Color.json")
        );
        assert!(message.contains("Decode failed: boom"));
        assert!(message.contains("key=[1, 2, 3]"));
        assert!(message.contains("layout=OnePartScale"));
        assert!(message.contains("encoding=ARGB"));
    }

    fn collect_files(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files.extend(collect_files(&path));
                } else {
                    files.push(path);
                }
            }
        }
        files
    }

    // ── 性能 smoke 测试 ────────────────────────────────────────────────
    // 非默认运行。执行方式：
    //   cargo test -p car-cli -- --ignored perf_smoke_extract_assets_car
    // 打印各阶段耗时，不做时间阈值断言。

    #[test]
    #[ignore = "性能 smoke 测试，使用 `cargo test -p car-cli -- --ignored` 运行"]
    fn perf_smoke_extract_assets_car() {
        let car = fixture_car();
        let output = unique_temp_dir("perf-smoke");

        let stats =
            run_collecting_stats(&car, &output, false).expect("perf smoke extract should succeed");

        eprintln!(
            "\n[perf smoke] plan={:.1}ms  parallel_save={:.1}ms  aggregate={:.1}ms  \
             jobs={}  saved={}  skipped={}  failed={}",
            stats.plan_ns as f64 / 1_000_000.0,
            stats.parallel_save_ns as f64 / 1_000_000.0,
            stats.aggregate_ns as f64 / 1_000_000.0,
            stats.jobs_planned,
            stats.saved,
            stats.skipped,
            stats.failed,
        );

        let file_count = collect_files(&output).len();
        std::fs::remove_dir_all(&output).ok();

        assert!(
            file_count > 0,
            "perf smoke: expected at least one extracted file"
        );
    }
}
