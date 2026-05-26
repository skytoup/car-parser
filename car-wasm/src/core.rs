use image::DynamicImage;
use image::ImageEncoder;

use car::{CSIItem, Car, ReferenceRect};

use crate::error::{WasmError, WasmResult};

pub(crate) fn load_car_from_bytes(bytes: Vec<u8>) -> WasmResult<Car> {
    Ok(Car::from_bytes(bytes)?)
}

pub(crate) fn resolved_source_bytes(car: &Car, item: &CSIItem) -> WasmResult<Vec<u8>> {
    Ok(car.resolved_source_bytes(item)?)
}

pub(crate) fn rgba_bytes_with_crops(
    item: &CSIItem,
    crops: &[ReferenceRect],
) -> WasmResult<Vec<u8>> {
    Ok(car::image::rgba_bytes_with_crops(item, crops)?.rgba)
}

pub(crate) fn thumbnail_png_bytes_with_crops(
    item: &CSIItem,
    crops: &[ReferenceRect],
    max_dimension: u32,
) -> WasmResult<Vec<u8>> {
    let rgba = car::image::rgba_bytes_with_crops(item, crops)?;
    let image =
        image::RgbaImage::from_raw(rgba.width, rgba.height, rgba.rgba).ok_or_else(|| {
            WasmError::decode_failed("decoded RGBA buffer size did not match thumbnail dimensions")
        })?;
    encode_thumbnail_png(DynamicImage::ImageRgba8(image), max_dimension)
}

pub(crate) fn thumbnail_png_bytes_from_source(
    source_bytes: &[u8],
    max_dimension: u32,
) -> WasmResult<Vec<u8>> {
    let image = image::load_from_memory(source_bytes)
        .map_err(|err| WasmError::decode_failed(err.to_string()))?;
    encode_thumbnail_png(image, max_dimension)
}

pub(crate) fn png_bytes_with_crops(item: &CSIItem, crops: &[ReferenceRect]) -> WasmResult<Vec<u8>> {
    Ok(car::image::png_bytes_with_crops(item, crops)?)
}

fn encode_thumbnail_png(image: DynamicImage, max_dimension: u32) -> WasmResult<Vec<u8>> {
    let max_dimension = max_dimension.max(1);
    let thumbnail = if image.width() > max_dimension || image.height() > max_dimension {
        image.resize(
            max_dimension,
            max_dimension,
            image::imageops::FilterType::Triangle,
        )
    } else {
        image
    };
    let rgba = thumbnail.to_rgba8();
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|err| WasmError::decode_failed(err.to_string()))?;
    Ok(bytes)
}
