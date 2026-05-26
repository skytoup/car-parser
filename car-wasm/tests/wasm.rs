use std::io::Cursor;

use image::ImageDecoder;
use serde::Deserialize;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

use car_wasm::{
    DisplayPayload, DownloadPayload, EntryInfo, EntryKind, EntryListItem, ImageInfo, ImageListItem,
    ThumbnailPayload, WasmArchive,
};

const ASSETS_CAR_BYTES: &[u8] = include_bytes!("../../car-tests/data/Assets.car");

#[derive(Debug, Deserialize)]
struct DocumentInfo {
    #[serde(rename = "RenditionCount")]
    rendition_count: u32,
    #[serde(rename = "MainVersionString")]
    main_version_string: String,
}

fn load_archive() -> WasmArchive {
    WasmArchive::from_bytes(ASSETS_CAR_BYTES.to_vec().into_boxed_slice())
        .expect("fixture archive should load")
}

fn display_payload_for(archive: &WasmArchive, id: &str) -> DisplayPayload {
    let payload = archive
        .get_display_payload(id)
        .unwrap_or_else(|err| panic!("display payload should succeed for {id}: {err:?}"));
    serde_wasm_bindgen::from_value(payload).expect("deserialize display payload")
}

fn download_payload_for(archive: &WasmArchive, id: &str) -> DownloadPayload {
    let payload = archive
        .get_download_payload(id)
        .unwrap_or_else(|err| panic!("download payload should succeed for {id}: {err:?}"));
    serde_wasm_bindgen::from_value(payload).expect("deserialize download payload")
}

fn thumbnail_payload_for(archive: &WasmArchive, id: &str) -> ThumbnailPayload {
    let payload = archive
        .get_thumbnail_payload(id, JsValue::UNDEFINED)
        .unwrap_or_else(|err| panic!("thumbnail payload should succeed for {id}: {err:?}"));
    serde_wasm_bindgen::from_value(payload).expect("deserialize thumbnail payload")
}

fn find_entry<'a>(
    list: &'a [EntryInfo],
    facet_name: &str,
    resolved_encoding: &str,
) -> &'a EntryInfo {
    list.iter()
        .find(|info| info.facet_name == facet_name && info.resolved_encoding == resolved_encoding)
        .unwrap_or_else(|| {
            panic!("missing entry for facet `{facet_name}` with encoding `{resolved_encoding}`")
        })
}

fn entry_list(archive: &WasmArchive) -> Vec<EntryInfo> {
    let list = archive.list_entries().expect("list entries");
    serde_wasm_bindgen::from_value(list).expect("entry list")
}

#[wasm_bindgen_test]
fn list_images_keeps_ids_stable_within_archive() {
    let archive = load_archive();

    let first = archive.list_images().expect("first list should succeed");
    let second = archive.list_images().expect("second list should succeed");

    let first: Vec<ImageInfo> = serde_wasm_bindgen::from_value(first).expect("first list");
    let second: Vec<ImageInfo> = serde_wasm_bindgen::from_value(second).expect("second list");

    assert!(
        !first.is_empty(),
        "fixture should expose at least one wasm image entry"
    );
    assert_eq!(first.len(), second.len());

    for (lhs, rhs) in first.iter().zip(second.iter()) {
        assert_eq!(lhs.id, rhs.id);
        assert_eq!(lhs.facet_name, rhs.facet_name);
        assert_eq!(lhs.rendition_name, rhs.rendition_name);
    }
}

#[wasm_bindgen_test]
fn list_entries_keeps_ids_stable_within_archive() {
    let archive = load_archive();

    let first = entry_list(&archive);
    let second = entry_list(&archive);

    assert!(
        !first.is_empty(),
        "fixture should expose at least one wasm entry"
    );
    assert_eq!(first.len(), second.len());

    for (lhs, rhs) in first.iter().zip(second.iter()) {
        assert_eq!(lhs.id, rhs.id);
        assert_eq!(lhs.facet_name, rhs.facet_name);
        assert_eq!(lhs.rendition_name, rhs.rendition_name);
    }
}

#[wasm_bindgen_test]
fn document_info_and_get_image_info_match_archive_contract() {
    let archive = load_archive();

    let document_info = archive.document_info().expect("document info");
    let document_info: DocumentInfo =
        serde_wasm_bindgen::from_value(document_info).expect("deserialize document info");
    assert!(document_info.rendition_count > 0);
    assert!(
        !document_info.main_version_string.is_empty(),
        "document info should carry core metadata"
    );

    let list = archive.list_images().expect("list images");
    let list: Vec<ImageInfo> = serde_wasm_bindgen::from_value(list).expect("list images");
    let first = list.first().expect("at least one image entry");

    let single = archive
        .get_image_info(&first.id)
        .expect("lookup by opaque id should succeed");
    let single: ImageInfo = serde_wasm_bindgen::from_value(single).expect("single image info");

    assert_eq!(first.id, single.id);
    assert_eq!(first.preview_source_id, single.preview_source_id);
    assert_eq!(first.facet_name, single.facet_name);
    assert_eq!(first.rendition_name, single.rendition_name);
    assert_eq!(first.preview_strategy, single.preview_strategy);
    assert_eq!(first.download_strategy, single.download_strategy);
}

#[wasm_bindgen_test]
fn document_info_and_get_entry_info_match_archive_contract() {
    let archive = load_archive();

    let document_info = archive.document_info().expect("document info");
    let document_info: DocumentInfo =
        serde_wasm_bindgen::from_value(document_info).expect("deserialize document info");
    assert!(document_info.rendition_count > 0);
    assert!(
        !document_info.main_version_string.is_empty(),
        "document info should carry core metadata"
    );

    let list = entry_list(&archive);
    let first = list.first().expect("at least one entry");

    let single = archive
        .get_entry_info(&first.id)
        .expect("lookup by opaque id should succeed");
    let single: EntryInfo = serde_wasm_bindgen::from_value(single).expect("single entry info");

    assert_eq!(first.id, single.id);
    assert_eq!(first.preview_source_id, single.preview_source_id);
    assert_eq!(first.facet_name, single.facet_name);
    assert_eq!(first.rendition_name, single.rendition_name);
    assert_eq!(first.entry_kind, single.entry_kind);
    assert_eq!(first.preview_strategy, single.preview_strategy);
    assert_eq!(first.download_strategy, single.download_strategy);
}

#[wasm_bindgen_test]
fn list_image_summaries_match_list_images_order_and_ids() {
    let archive = load_archive();
    let full = archive.list_images().expect("list images");
    let full: Vec<ImageInfo> = serde_wasm_bindgen::from_value(full).expect("full image list");

    let summaries = archive
        .list_image_summaries()
        .expect("list image summaries");
    let summaries: Vec<ImageListItem> =
        serde_wasm_bindgen::from_value(summaries).expect("summary image list");

    assert_eq!(full.len(), summaries.len());

    for (full, summary) in full.iter().zip(summaries.iter()) {
        assert_eq!(full.id, summary.id);
        assert_eq!(full.facet_name, summary.facet_name);
        assert_eq!(full.rendition_name, summary.rendition_name);
        assert_eq!(full.width, summary.width);
        assert_eq!(full.height, summary.height);
        assert_eq!(full.scale, summary.scale);
        assert_eq!(full.resolved_encoding, summary.resolved_encoding);
        assert_eq!(full.preview_source_id, summary.preview_source_id);
        assert_eq!(full.preview_strategy, summary.preview_strategy);
    }
}

#[wasm_bindgen_test]
fn list_entry_summaries_match_list_entries_order_and_ids() {
    let archive = load_archive();
    let full = entry_list(&archive);

    let summaries = archive
        .list_entry_summaries()
        .expect("list entry summaries");
    let summaries: Vec<EntryListItem> =
        serde_wasm_bindgen::from_value(summaries).expect("summary entry list");

    assert_eq!(full.len(), summaries.len());

    for (full, summary) in full.iter().zip(summaries.iter()) {
        assert_eq!(full.id, summary.id);
        assert_eq!(full.facet_name, summary.facet_name);
        assert_eq!(full.rendition_name, summary.rendition_name);
        assert_eq!(full.width, summary.width);
        assert_eq!(full.height, summary.height);
        assert_eq!(full.scale, summary.scale);
        assert_eq!(full.resolved_encoding, summary.resolved_encoding);
        assert_eq!(full.entry_kind, summary.entry_kind);
        assert_eq!(full.preview_source_id, summary.preview_source_id);
        assert_eq!(full.preview_strategy, summary.preview_strategy);
        assert_eq!(full.downloadable, summary.downloadable);
    }
}

#[wasm_bindgen_test]
fn list_entries_include_color_and_raw_data_while_legacy_images_filter_them_out() {
    let archive = load_archive();
    let entries = entry_list(&archive);

    assert!(
        entries
            .iter()
            .any(|entry| entry.entry_kind == EntryKind::Color),
        "entry list should include color resources",
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.entry_kind == EntryKind::RawData),
        "entry list should include raw-data resources",
    );

    let legacy_images = archive.list_images().expect("list images");
    let legacy_images: Vec<ImageInfo> =
        serde_wasm_bindgen::from_value(legacy_images).expect("legacy image list");

    assert!(
        legacy_images
            .iter()
            .all(|entry| entry.entry_kind != EntryKind::Color
                && entry.entry_kind != EntryKind::RawData),
        "legacy image list should remain a supported-visual subset",
    );
    assert!(
        entries.len() > legacy_images.len(),
        "full entry list should contain more resources than the legacy image list",
    );
}

#[wasm_bindgen_test]
#[ignore = "smoke fixture 没有 PDF/HEIF paired-preview 资源"]
fn raw_data_download_only_entries_expose_original_downloads() {
    let archive = load_archive();
    let list = entry_list(&archive);
    let raw_data = find_entry(&list, "wcpayfacehbBo", "data");

    assert_eq!(
        raw_data.preview_strategy,
        car_wasm::PreviewStrategy::DownloadOnly
    );
    assert!(matches!(
        display_payload_for(&archive, &raw_data.id),
        DisplayPayload::DownloadOnly { .. }
    ));

    let download = download_payload_for(&archive, &raw_data.id);
    assert_eq!(
        download.download_strategy,
        car_wasm::DownloadStrategy::Original
    );
    assert_eq!(download.suggested_extension, "m4a");
    assert!(!download.bytes.is_empty());
}

#[wasm_bindgen_test]
fn missing_id_returns_typed_error() {
    let archive = load_archive();
    let err = archive
        .get_image_info("missing-entry")
        .expect_err("unknown id should fail");

    let payload: serde_json::Value =
        serde_wasm_bindgen::from_value(err).expect("typed error payload");
    assert_eq!(payload["code"], "EntryNotFound");
}

#[wasm_bindgen_test]
fn color_entries_render_color_swatch_and_reject_downloads() {
    let archive = load_archive();
    let color_entry = entry_list(&archive)
        .into_iter()
        .find(|entry| entry.entry_kind == EntryKind::Color)
        .expect("fixture should expose a color entry");

    let display = archive
        .get_display_payload(&color_entry.id)
        .expect("color display payload should succeed");
    let display: DisplayPayload =
        serde_wasm_bindgen::from_value(display).expect("deserialize color display payload");
    match display {
        DisplayPayload::ColorSwatch {
            color_space,
            components,
            css_color,
        } => {
            assert!(!color_space.is_empty());
            assert!(!components.is_empty());
            assert!(!css_color.is_empty());
        }
        other => panic!("expected color-swatch payload, got {other:?}"),
    }

    let err = archive
        .get_download_payload(&color_entry.id)
        .expect_err("color entries should not be downloadable");
    let payload: serde_json::Value =
        serde_wasm_bindgen::from_value(err).expect("typed error payload");
    assert_eq!(payload["code"], "EntryNotDownloadable");
}

#[wasm_bindgen_test]
fn payload_methods_follow_declared_strategy() {
    let archive = load_archive();
    let list = archive.list_images().expect("list images");
    let list: Vec<ImageInfo> = serde_wasm_bindgen::from_value(list).expect("list images");

    let binary = list
        .iter()
        .find(|info| info.preview_strategy == car_wasm::PreviewStrategy::ImgBinary)
        .expect("fixture should expose at least one browser-binary entry");
    let display = archive
        .get_display_payload(&binary.id)
        .expect("binary display payload should succeed");
    let display: DisplayPayload =
        serde_wasm_bindgen::from_value(display).expect("deserialize binary display payload");
    match display {
        DisplayPayload::ImgBinary { bytes, .. } => assert!(!bytes.is_empty()),
        other => panic!("expected img-binary payload, got {other:?}"),
    }

    let raster = list
        .iter()
        .find(|info| info.download_strategy == car_wasm::DownloadStrategy::Png)
        .expect("fixture should expose at least one raster->png entry");
    let raster_display = archive
        .get_display_payload(&raster.id)
        .expect("raster display payload should succeed");
    let raster_display: DisplayPayload =
        serde_wasm_bindgen::from_value(raster_display).expect("deserialize raster display payload");
    match raster_display {
        DisplayPayload::CanvasRgba {
            width,
            height,
            rgba,
            ..
        } => {
            assert_eq!(rgba.len(), (width * height * 4) as usize);
        }
        other => panic!("expected canvas-rgba payload, got {other:?}"),
    }

    let download = archive
        .get_download_payload(&raster.id)
        .expect("raster download payload should succeed");
    let download: DownloadPayload =
        serde_wasm_bindgen::from_value(download).expect("deserialize raster download payload");
    assert!(!download.bytes.is_empty());
    assert_eq!(download.suggested_extension, "png");
    assert!(!download.suggested_file_name.is_empty());
}

#[wasm_bindgen_test]
fn display_gamut_deepmap2_entries_render_canvas_rgba_in_wasm() {
    let archive = load_archive();
    let entries = entry_list(&archive);
    let display_gamut_entries: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry.facet_name == "AS_YuanBao_Dark"
                && entry.resolved_encoding == "argb16"
                && entry.preview_strategy == car_wasm::PreviewStrategy::CanvasRgba
        })
        .collect();

    assert!(
        display_gamut_entries.len() >= 3,
        "fixture should expose display-gamut canvas entries"
    );

    for entry in display_gamut_entries {
        let display = display_payload_for(&archive, &entry.id);
        match display {
            DisplayPayload::CanvasRgba {
                width,
                height,
                rgba,
                ..
            } => {
                assert_eq!(width, entry.width, "{} width", entry.id);
                assert_eq!(height, entry.height, "{} height", entry.id);
                assert_eq!(
                    rgba.len(),
                    (width * height * 4) as usize,
                    "{} rgba bytes",
                    entry.id
                );
            }
            other => panic!(
                "expected canvas-rgba payload for {}, got {other:?}",
                entry.id
            ),
        }
    }
}

#[wasm_bindgen_test]
fn display_gamut_deepmap2_entry_downloads_png_in_wasm() {
    let archive = load_archive();
    let entry = entry_list(&archive)
        .into_iter()
        .find(|entry| {
            entry.facet_name == "AS_YuanBao_Dark"
                && entry.resolved_encoding == "argb16"
                && entry.download_strategy == car_wasm::DownloadStrategy::Png
        })
        .expect("fixture should expose a display-gamut download-as-png entry");
    let download = download_payload_for(&archive, &entry.id);

    assert_eq!(download.download_strategy, car_wasm::DownloadStrategy::Png);
    assert_eq!(download.suggested_extension, "png");
    assert_eq!(download.mime_type, "image/png");
    assert!(
        download.bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "download should be encoded as PNG bytes"
    );
}

#[wasm_bindgen_test]
fn raster_thumbnail_payloads_are_png_and_fit_within_default_dimension() {
    let archive = load_archive();
    let list = archive.list_images().expect("list images");
    let list: Vec<ImageInfo> = serde_wasm_bindgen::from_value(list).expect("list images");
    let raster = list
        .iter()
        .find(|info| info.preview_strategy == car_wasm::PreviewStrategy::CanvasRgba)
        .expect("fixture should expose at least one raster entry");

    let thumbnail = thumbnail_payload_for(&archive, &raster.id);
    match thumbnail {
        ThumbnailPayload::ImgBinary { mime_type, bytes } => {
            assert_eq!(mime_type, "image/png");
            let decoder =
                image::codecs::png::PngDecoder::new(Cursor::new(bytes)).expect("decode png");
            let (width, height) = decoder.dimensions();
            assert!(width <= 256, "thumbnail width should be capped");
            assert!(height <= 256, "thumbnail height should be capped");

            if raster.width != raster.height {
                let original_ratio = raster.width as f64 / raster.height as f64;
                let thumbnail_ratio = width as f64 / height as f64;
                assert!(
                    (original_ratio - thumbnail_ratio).abs() < 0.05,
                    "thumbnail should preserve aspect ratio"
                );
            }
        }
        other => panic!("expected binary thumbnail payload, got {other:?}"),
    }
}

#[wasm_bindgen_test]
fn download_only_thumbnail_payloads_return_placeholder_without_bytes() {
    let archive = load_archive();
    let list = entry_list(&archive);
    let download_only = list
        .iter()
        .find(|info| info.preview_strategy == car_wasm::PreviewStrategy::DownloadOnly)
        .expect("fixture should expose at least one download-only entry");

    let thumbnail = thumbnail_payload_for(&archive, &download_only.id);
    assert_eq!(thumbnail, ThumbnailPayload::DownloadOnly);
}

#[wasm_bindgen_test]
fn thumbnail_payload_requests_are_stable_within_archive_cache() {
    let archive = load_archive();
    let list = archive.list_images().expect("list images");
    let list: Vec<ImageInfo> = serde_wasm_bindgen::from_value(list).expect("list images");
    let target = list
        .iter()
        .find(|info| {
            matches!(
                info.preview_strategy,
                car_wasm::PreviewStrategy::CanvasRgba | car_wasm::PreviewStrategy::ImgBinary
            )
        })
        .expect("fixture should expose at least one thumbnail-capable entry");

    let first = thumbnail_payload_for(&archive, &target.id);
    let second = thumbnail_payload_for(&archive, &target.id);
    assert_eq!(first, second);
}
