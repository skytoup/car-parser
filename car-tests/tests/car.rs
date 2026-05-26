mod util;

use std::io::Cursor;

use car::rendition::Rendition;
use deku::prelude::*;

use util::{TestResult, fetch_csiitem_with_encoding, smoke_fixture};

const SMOKE_IMAGE: &str = "2016_coin1";
const SMOKE_COLOR: &str = "ActionSheet_Action_Icon_Color";
const SMOKE_DOTTED_COLOR: &str = "ActionSheet_Action_Icon_Color.ease";

#[test]
fn decode_rendition_data() -> TestResult {
    let car = car::Car::new(smoke_fixture("Assets.car"))?;

    let mut decoded_count = 0usize;
    for facet in car.facets() {
        let Some(items) = car.rendtions_with_facet(facet) else {
            continue;
        };
        for item in items {
            match item.decode_data() {
                Ok(Some(_)) => decoded_count += 1,
                Ok(None) => {}
                Err(car::CarError::UnsupportedCompression(_)) => {}
                Err(car::CarError::Deepmap2(_)) => {}
                Err(e) => panic!("decode_data failed unexpectedly: {}", e),
            }
        }
    }

    assert!(
        decoded_count > 0,
        "Expected at least one rendition to decode successfully"
    );
    Ok(())
}

#[test]
fn decode_data_ref_and_owned_match_for_smoke_asset() -> TestResult {
    let car = car::Car::new(smoke_fixture("Assets.car"))?;
    let item = car
        .rendtions_with_name(SMOKE_IMAGE)
        .and_then(|items| items.first())
        .expect("missing smoke image rendition");

    let borrowed = item.decode_data_ref()?.expect("borrowed rendition data");
    let owned = item.decode_data_owned()?.expect("owned rendition data");

    match (borrowed, owned) {
        (car::RenditionDataRef::Image { data: borrowed }, car::RenditionData::Image { data }) => {
            assert_eq!(borrowed.as_ref(), data.as_slice());
        }
        other => panic!("unexpected rendition pair: {other:?}"),
    }

    Ok(())
}

#[test]
fn read_csi_item() -> TestResult {
    let car = car::Car::new(smoke_fixture("Assets.car"))?;

    for facet in car.facets() {
        car.rendtions_with_facet(facet)
            .expect("Can not find facet in car");
    }

    Ok(())
}

#[test]
fn read_rendition_key_fmt() -> TestResult {
    let car = car::Car::new(smoke_fixture("Assets.car"))?;

    let ver = 0_u32;
    let count = 12_u32;
    let attrs: Vec<_> = vec![7_u16, 13, 12, 15, 16, 9, 8, 24, 21, 17, 1, 2]
        .into_iter()
        .map(|n| {
            let mut r = Reader::new(Cursor::new(n.to_le_bytes()));
            car::rendition::AttributeType::from_reader_with_ctx(&mut r, deku::ctx::Endian::Little)
                .unwrap()
        })
        .collect();

    let key_fmt = car.key_format();

    assert_eq!(ver, key_fmt.version);
    assert_eq!(count, key_fmt.max_count);
    assert_eq!(attrs, key_fmt.attribute_types);

    Ok(())
}

#[test]
fn read_header() -> TestResult {
    let car = car::Car::new(smoke_fixture("Assets.car"))?;

    let coreui_version = 971_u32;
    let storage_version = 17_u32;
    let storage_timestamp = 0_u32;
    let rendition_count = 4964_u32;
    let main_version_string = "@(#)PROGRAM:CoreUI  PROJECT:CoreUI-971.6";
    let version_string = "Xcode 26.1.1 (17B100) via AssetCatalogSimulatorAgent";
    let uuid = [0_u8; 16];
    let atassociated_checksumag = 0_u32;
    let schema_version = 2_u32;
    let color_space = car::ColorSpace::SRGB;
    let key_semantics = 2_u32;

    let header = car.header();

    assert_eq!(coreui_version, header.coreui_version);
    assert_eq!(storage_version, header.storage_version);
    assert_eq!(storage_timestamp, header.storage_timestamp);
    assert_eq!(rendition_count, header.rendition_count);
    assert_eq!(main_version_string, header.main_version_string);
    assert_eq!(version_string, header.version_string);
    assert_eq!(uuid, header.uuid);
    assert_eq!(atassociated_checksumag, header.associated_checksumag);
    assert_eq!(schema_version, header.schema_version);
    assert_eq!(color_space, header.color_space);
    assert_eq!(key_semantics, header.key_semantics);

    Ok(())
}

#[test]
fn read_ext_metadta() -> TestResult {
    let car = car::Car::new(smoke_fixture("Assets.car"))?;

    let thin_args = b"";
    let dev_pf = b"ios";
    let dev_pf_ver = b"15.0";
    let a_tool =
        b"@(#)PROGRAM:CoreThemeDefinition  PROJECT:CoreThemeDefinition-653.2  [IIO-2784.1.3.3]";

    let data = car.extended_metadata();

    assert_eq!(thin_args, data.thinning_args.as_bytes());
    assert_eq!(dev_pf, data.deployment_platform.as_bytes());
    assert_eq!(dev_pf_ver, data.deployment_platform_version.as_bytes());
    assert_eq!(a_tool, data.authoring_tool.as_bytes());

    Ok(())
}

#[test]
fn read_tree_appearance_keys() -> TestResult {
    let car = car::Car::new(smoke_fixture("Assets.car"))?;

    println!("{:?}", car.appearance_keys());

    Ok(())
}

#[test]
fn read_facet_keys() -> TestResult {
    let car = car::Car::new(smoke_fixture("Assets.car"))?;

    println!("{:?}", car.facets());

    Ok(())
}

#[test]
fn parse_color() -> TestResult {
    let car = car::Car::new(smoke_fixture("Assets.car"))?;
    let params = vec![
        (
            SMOKE_COLOR,
            car::ColorSpace::SRGB,
            vec![0.0, 0.0, 0.0, 0.8980000019073486],
        ),
        (
            SMOKE_DOTTED_COLOR,
            car::ColorSpace::SRGB,
            vec![0.0, 0.0, 0.0, 1.0],
        ),
        (
            "Button_Negative_BG",
            car::ColorSpace::SRGB,
            vec![
                0.9800000190734863,
                0.3179999887943268,
                0.3179999887943268,
                1.0,
            ],
        ),
        (
            "Button_Primary_BG",
            car::ColorSpace::SRGB,
            vec![
                0.027000000700354576,
                0.7570000290870667,
                0.37599998712539673,
                1.0,
            ],
        ),
    ];

    for param in params {
        let color = find_color(&car, param.0).expect(&format!("Can not found {:?}", param));

        assert_eq!(param.1, color.color_space, "{:?}", param);
        assert_eq!(param.2, color.components, "{:?}", param);
    }

    Ok(())
}

fn find_color<'a>(
    car: &'a car::Car,
    facet_name: &str,
) -> Option<&'a car::rendition::RenditionColor> {
    fetch_csiitem_with_encoding(car, facet_name, car::Encoding::None)
        .and_then(|item| item.header().rendition.as_ref())
        .and_then(|rendition| match rendition {
            Rendition::Color(color) => Some(color),
            _ => None,
        })
}
