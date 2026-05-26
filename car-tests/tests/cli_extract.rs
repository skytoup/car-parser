mod util;

use std::path::{Path, PathBuf};

use car::export::should_save_raw_item;
use car::{CarError, rendition::Rendition};
use test_support::temp_output_dir;

use util::{TestResult, smoke_fixture};

#[test]
fn decode_rendition_data_to_files() -> TestResult {
    let output_fp = temp_output_dir("car-tests-car-decode");
    let car = car::Car::new(smoke_fixture("Assets.car"))?;
    let mut saved_count = 0usize;
    let mut skipped_count = 0usize;

    if output_fp.exists() {
        std::fs::remove_dir_all(&output_fp)?;
    }
    std::fs::create_dir_all(&output_fp)?;

    for facet in car.facets() {
        let Some(items) = car.rendtions_with_facet(facet) else {
            continue;
        };
        for item in items {
            let Some(fp) = export_path_for_item(&output_fp, item) else {
                continue;
            };
            if let Some(parent) = fp.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let result = match item.header().rendition.as_ref() {
                Some(Rendition::RawData(_)) => car::image::save_raw(item, &fp),
                Some(Rendition::ThemeCBCK(_)) => {
                    if should_save_raw_item(item) {
                        car::image::save_raw(item, &fp)
                    } else {
                        car::image::save_image(item, &fp)
                    }
                }
                _ => continue,
            };

            match result {
                Ok(()) => saved_count += 1,
                Err(CarError::UnsupportedCompression(_))
                | Err(CarError::UnsupportedEncoding(_))
                | Err(CarError::Deepmap2(_))
                | Err(CarError::DecodeFailed(_)) => {
                    skipped_count += 1;
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    assert!(
        saved_count > 0,
        "Expected at least one rendition file to be saved"
    );
    eprintln!("saved {saved_count} files, skipped {skipped_count} undecodable renditions");

    Ok(())
}

fn export_path_for_item(output_root: &Path, item: &car::CSIItem) -> Option<PathBuf> {
    let mut fp = output_root.join(item.name());
    match item.header().rendition.as_ref()? {
        Rendition::RawData(_) => ensure_raw_extension(&mut fp, item),
        Rendition::ThemeCBCK(_) if should_save_raw_item(item) => {
            ensure_raw_extension(&mut fp, item)
        }
        Rendition::ThemeCBCK(_) => ensure_image_extension(&mut fp),
        _ => return None,
    }
    Some(fp)
}

fn ensure_raw_extension(fp: &mut PathBuf, item: &car::CSIItem) {
    if fp.extension().is_none() {
        fp.set_extension(car::default_raw_extension_for_item(item));
    }
}

fn ensure_image_extension(fp: &mut PathBuf) {
    let supported = fp
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .is_some_and(|ext| matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp"));

    if !supported {
        fp.set_extension("png");
    }
}
