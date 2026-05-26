//! Stable image-oriented helpers for [`crate::CSIItem`].
//!
//! This module keeps optional image functionality out of the core parser API.
//! Callers can decode or save image-like renditions through free functions
//! instead of depending on feature-gated inherent methods.
//!
//! Typical usage:
//!
//! ```no_run
//! # #[cfg(feature = "image")]
//! # fn demo(item: &car::CSIItem) -> Result<(), car::CarError> {
//! let image = car::image::to_image(item)?;
//! car::image::save_image(item, "/tmp/example.png")?;
//! let _ = image;
//! # Ok(())
//! # }
//! ```

use std::path::Path;

use crate::car::{CSIItem, CarError, ReferenceRect};
use crate::decode::DecodeOptions;
use crate::image_conv::RgbaPayload;

pub type CarImageError = CarError;

pub fn to_image(item: &CSIItem) -> Result<image::DynamicImage, CarImageError> {
    item.to_image()
}

pub fn to_image_with_options(
    item: &CSIItem,
    options: &DecodeOptions,
) -> Result<image::DynamicImage, CarImageError> {
    item.to_image_with_options(options)
}

pub fn save_image(item: &CSIItem, path: impl AsRef<Path>) -> Result<(), CarImageError> {
    item.save_image(path)
}

pub fn save_image_with_crops(
    item: &CSIItem,
    path: impl AsRef<Path>,
    crops: &[ReferenceRect],
) -> Result<(), CarImageError> {
    item.save_image_with_crops(path, crops)
}

pub fn save_raw(item: &CSIItem, path: impl AsRef<Path>) -> Result<(), CarImageError> {
    item.save_raw(path)
}

pub fn rgba_bytes(item: &CSIItem) -> Result<RgbaPayload, CarImageError> {
    item.rgba_bytes()
}

pub fn rgba_bytes_with_crops(
    item: &CSIItem,
    crops: &[ReferenceRect],
) -> Result<RgbaPayload, CarImageError> {
    item.rgba_bytes_with_crops(crops)
}

pub fn png_bytes(item: &CSIItem) -> Result<Vec<u8>, CarImageError> {
    item.png_bytes()
}

pub fn png_bytes_with_crops(
    item: &CSIItem,
    crops: &[ReferenceRect],
) -> Result<Vec<u8>, CarImageError> {
    item.png_bytes_with_crops(crops)
}
