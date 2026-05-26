mod util;

use car::rendition::{AttributeType, LayoutType};
use image::ImageDecoder;
use test_support::temp_output_dir;

use util::{TestResult, full_fixture, load_full_car, smoke_fixture};

fn fixture_car() -> car::Car {
    car::Car::new(smoke_fixture("Assets.car")).expect("load test Assets.car")
}

fn attr_value(item: &car::CSIItem, attr_type: AttributeType) -> Option<u16> {
    item.attributes()
        .iter()
        .find(|attr| attr.name == attr_type)
        .map(|attr| attr.val)
}

#[test]
fn coin_png_decodes_to_fixture_dimensions() -> TestResult {
    let car = fixture_car();
    let item = car
        .rendtions_with_name("2016_coin1")
        .and_then(|items| items.first())
        .expect("missing 2016_coin1 rendition");

    let decoded = car::image::to_image(item)?;

    assert_eq!((decoded.width(), decoded.height()), (270, 270));
    Ok(())
}

#[test]
fn airkiss_png_decodes_to_fixture_dimensions() -> TestResult {
    let car = fixture_car();
    let item = car
        .rendtions_with_name("AirKissHelper")
        .and_then(|items| items.first())
        .expect("missing AirKissHelper rendition");

    let decoded = car::image::to_image(item)?;
    assert_eq!((decoded.width(), decoded.height()), (510, 510));
    Ok(())
}

#[test]
fn display_gamut_renditions_decode_to_full_size() -> TestResult {
    let car = fixture_car();
    let items = car
        .rendtions_with_name("AS_YuanBao_Dark")
        .expect("missing AS_YuanBao_Dark renditions");

    let display_gamut_items: Vec<_> = items
        .iter()
        .filter(|item| attr_value(item, AttributeType::DisplayGamut) == Some(1))
        .collect();

    assert_eq!(display_gamut_items.len(), 3);
    for item in display_gamut_items {
        let resolved = car
            .resolve_internal_reference(item)
            .expect("display-gamut InternalReference should resolve");
        let path = temp_output_dir("car-tests-display-gamut").join(format!(
            "{}-{}x.png",
            std::thread::current().name().unwrap_or("test"),
            attr_value(item, AttributeType::Scale).unwrap_or_default()
        ));
        std::fs::create_dir_all(path.parent().expect("temp output should have parent"))?;
        car::image::save_image_with_crops(resolved.source, &path, &resolved.crops)?;
        let img = image::open(&path)?;
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            (img.width(), img.height()),
            (item.width(), item.height()),
            "scale={} encoding={:?}",
            attr_value(item, AttributeType::Scale).unwrap_or_default(),
            item.header().encoding
        );
    }

    Ok(())
}

#[test]
fn display_gamut_rendition_embeds_display_p3_icc() -> TestResult {
    let car = fixture_car();
    let item = car
        .rendtions_with_name("AS_YuanBao_Dark")
        .and_then(|items| {
            items
                .iter()
                .find(|item| attr_value(item, AttributeType::DisplayGamut) == Some(1))
        })
        .expect("missing AS_YuanBao_Dark Display P3 rendition");
    let resolved = car
        .resolve_internal_reference(item)
        .expect("display-gamut InternalReference should resolve");

    let path = temp_output_dir("car-tests-p3").join(format!(
        "{}.png",
        std::thread::current().name().unwrap_or("test")
    ));
    std::fs::create_dir_all(path.parent().expect("temp output should have parent"))?;
    let expected_profile = std::fs::read("/System/Library/ColorSync/Profiles/Display P3.icc")?;

    car::image::save_image_with_crops(resolved.source, &path, &resolved.crops)?;
    let reader = std::io::BufReader::new(std::fs::File::open(&path)?);
    let mut decoder = image::codecs::png::PngDecoder::new(reader)?;
    let actual_profile = decoder
        .icc_profile()?
        .expect("expected embedded Display P3 ICC profile");
    assert_eq!(actual_profile, expected_profile);

    std::fs::remove_file(path)?;
    Ok(())
}

fn fixture_ios_car() -> Option<car::Car> {
    load_full_car("Assets_iOS.car")
}

#[test]
fn internal_reference_animate_tag_newoutfit_has_opaque_pixels() -> TestResult {
    let Some(car) = fixture_ios_car() else {
        return Ok(());
    };
    let Some(system_extract_root) = full_fixture("Assets_iOS_system_extract") else {
        return Ok(());
    };

    let item = car
        .rendtions_with_name("animate_tag_newoutfit")
        .and_then(|items| {
            items
                .iter()
                .find(|i| matches!(i.layout(), LayoutType::InternalReference))
        })
        .expect("animate_tag_newoutfit InternalReference rendition");

    let resolved = car
        .resolve_internal_reference(item)
        .expect("resolve_internal_reference should succeed");
    let source = resolved.source;

    let path = temp_output_dir("car-tests-animate-tag").join(format!(
        "{}.png",
        std::thread::current().name().unwrap_or("test")
    ));
    std::fs::create_dir_all(path.parent().expect("temp output should have parent"))?;
    car::image::save_image_with_crops(source, &path, &resolved.crops)?;

    let decoded = image::open(&path)?.to_rgba8();
    let _ = std::fs::remove_file(&path);
    let system = image::open(system_extract_root.join("animate_tag_newoutfit@3x.png"))?.to_rgba8();

    assert_eq!(
        decoded.width(),
        item.width(),
        "width should equal InternalReference logical width"
    );
    assert_eq!(
        decoded.height(),
        item.height(),
        "height should equal InternalReference logical height"
    );
    assert_eq!(
        decoded.dimensions(),
        system.dimensions(),
        "system extract dimensions should match"
    );

    let opaque_count = decoded.pixels().filter(|p| p[3] > 0).count();
    assert!(
        opaque_count > 0,
        "animate_tag_newoutfit@3x must have non-transparent pixels after atlas crop, got {opaque_count}"
    );

    let pixel_mismatches = decoded
        .pixels()
        .zip(system.pixels())
        .filter(|(actual, expected)| actual.0 != expected.0)
        .count();
    assert_eq!(
        pixel_mismatches, 0,
        "RGBA pixels should match system extract exactly, mismatched pixels: {pixel_mismatches}"
    );

    Ok(())
}
