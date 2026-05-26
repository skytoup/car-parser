use anyhow::Result;
use car::Car;

pub fn run(car: &Car) -> Result<()> {
    let mut entries = Vec::new();
    entries.push(serde_json::to_value(car.document_info())?);
    for rendition in car.rendition_infos() {
        entries.push(serde_json::to_value(rendition)?);
    }

    serde_json::to_writer_pretty(std::io::stdout(), &entries)?;
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use car::Car;
    use serde_json::Value;
    use test_support::fixture_group;

    fn fixture_car() -> Car {
        let group = fixture_group("smoke");
        Car::new(group.path("Assets.car")).expect("load test Assets.car")
    }

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

    #[test]
    fn info_uses_attribute_scale_when_header_scale_is_zero() {
        let car = fixture_car();
        let entry = car
            .rendition_infos()
            .into_iter()
            .find(|entry| entry.name == "ActionSheet_Action_Icon_Color")
            .expect("smoke color rendition");
        let json = serde_json::to_value(entry).expect("serialize rendition info");

        assert_eq!(json["Scale"], 1);
        assert_eq!(json["AttributeScale"], 1);
    }

    #[test]
    fn info_internal_reference_uses_source_asset_type_and_compression() {
        let Some(car) = fixture_ios_car() else {
            return;
        };
        let entry = car
            .rendition_infos()
            .into_iter()
            .find(|entry| {
                entry.name == "animate_tag_newoutfit" && entry.layout == "InternalReference"
            })
            .expect("animate_tag_newoutfit InternalReference rendition");
        let json: Value = serde_json::to_value(entry).expect("serialize rendition info");

        assert_eq!(json["AssetType"], "Image");
        assert_eq!(json["Compression"], "Deepmap2");
        assert_eq!(json["Layout"], "InternalReference");
        assert_eq!(json["Opaque"], false);
    }
}
