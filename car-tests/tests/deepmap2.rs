mod util;

use car::rendition::Rendition;

use util::{TestResult, smoke_fixture};

#[test]
fn decode_deepmap2_rendition_from_assets_car() -> TestResult {
    let car = car::Car::new(smoke_fixture("Assets.car"))?;

    let payloads = find_all_deepmap2_payloads(&car);
    assert!(
        !payloads.is_empty(),
        "no deepmap2 payloads found in Assets.car"
    );

    let total_raw = payloads
        .iter()
        .filter(|p| matches!(p, Deepmap2Payload::Raw(_)))
        .count();
    let total_kcbc = payloads
        .iter()
        .filter(|p| matches!(p, Deepmap2Payload::Kcbc(_)))
        .count();

    let mut decoded_raw = 0usize;
    let mut decoded_kcbc = 0usize;
    let mut skipped_raw = 0usize;
    let mut skipped_kcbc = 0usize;

    for payload in &payloads {
        match payload {
            Deepmap2Payload::Raw(raw) => match deepmap2::decode(raw) {
                Ok(image) => {
                    assert_decoded_image(image);
                    decoded_raw += 1;
                }
                Err(e) if is_skippable_deepmap2_error(&e) => {
                    skipped_raw += 1;
                }
                Err(e) => return Err(e.into()),
            },
            Deepmap2Payload::Kcbc(kcbc) => match deepmap2::decode_kcbc(kcbc) {
                Ok(image) => {
                    assert_decoded_image(image);
                    decoded_kcbc += 1;

                    match deepmap2::tile::parse_kcbc(kcbc) {
                        Ok(tiles) => {
                            if let Some(raw_tile) = tiles.first() {
                                match deepmap2::decode(raw_tile) {
                                    Ok(image) => assert_decoded_image(image),
                                    Err(e) if is_skippable_deepmap2_error(&e) => {}
                                    Err(e) => return Err(e.into()),
                                }
                            }
                        }
                        Err(e) if is_skippable_deepmap2_error(&e) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                Err(e) if is_skippable_deepmap2_error(&e) => {
                    skipped_kcbc += 1;
                }
                Err(e) => return Err(e.into()),
            },
        }
    }

    eprintln!(
        "deepmap2 coverage: raw={decoded_raw}/{total_raw}, kcbc={decoded_kcbc}/{total_kcbc}, \
         skipped_raw={skipped_raw}, skipped_kcbc={skipped_kcbc}, total={}",
        payloads.len()
    );
    assert!(
        decoded_raw + decoded_kcbc > 0,
        "no deepmap2 payloads were successfully decoded"
    );
    assert!(
        total_kcbc == 0 || decoded_kcbc > 0,
        "found {total_kcbc} KCBC payloads but none decoded successfully \
         (skipped={skipped_kcbc})"
    );

    Ok(())
}

fn is_skippable_deepmap2_error(error: &deepmap2::Deepmap2Error) -> bool {
    matches!(
        error,
        deepmap2::Deepmap2Error::UnsupportedPixelFormat(_)
            | deepmap2::Deepmap2Error::UnsupportedDecodeType(_)
            | deepmap2::Deepmap2Error::InvalidData(_)
            | deepmap2::Deepmap2Error::LzfseDecompress
    )
}

fn assert_decoded_image(image: deepmap2::DecodedImage) {
    assert!(image.width > 0);
    assert!(image.height > 0);
    assert_eq!(
        image.rgba.len(),
        image.width as usize * image.height as usize * 4
    );
}

enum Deepmap2Payload<'a> {
    Raw(&'a [u8]),
    Kcbc(&'a [u8]),
}

fn find_all_deepmap2_payloads(car: &car::Car) -> Vec<Deepmap2Payload<'_>> {
    let mut payloads = Vec::new();

    for facet in car.facets() {
        let Some(items) = car.rendtions_with_facet(facet) else {
            continue;
        };

        for item in items {
            let Some(rendition) = item.header().rendition.as_ref() else {
                continue;
            };

            match rendition {
                Rendition::RawData(raw) => {
                    let raw_data = raw.raw_data.as_slice();
                    if raw_data.starts_with(b"KCBC") {
                        payloads.push(Deepmap2Payload::Kcbc(raw_data));
                    } else if raw_data.starts_with(b"dmp2") {
                        payloads.push(Deepmap2Payload::Raw(raw_data));
                    } else if let Some(offset) = deepmap2::tile::find_bytes(raw_data, b"KCBC") {
                        payloads.push(Deepmap2Payload::Kcbc(&raw_data[offset..]));
                    } else if let Some(offset) = deepmap2::tile::find_bytes(raw_data, b"dmp2") {
                        payloads.push(Deepmap2Payload::Raw(&raw_data[offset..]));
                    }
                }
                Rendition::ThemeCBCK(theme) => {
                    for raw in theme.raw_datas() {
                        if raw.starts_with(b"KCBC") {
                            payloads.push(Deepmap2Payload::Kcbc(raw));
                        } else if let Some(offset) = deepmap2::tile::find_bytes(raw, b"KCBC") {
                            payloads.push(Deepmap2Payload::Kcbc(&raw[offset..]));
                        } else if raw.starts_with(b"dmp2") {
                            payloads.push(Deepmap2Payload::Raw(raw));
                        } else if let Some(offset) = deepmap2::tile::find_bytes(raw, b"dmp2") {
                            payloads.push(Deepmap2Payload::Raw(&raw[offset..]));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    payloads
}
