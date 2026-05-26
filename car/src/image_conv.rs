use std::borrow::Cow;
use std::path::Path;

use crate::car::{CSIItem, CarError, CarResult};
#[cfg(feature = "image")]
use crate::decode::{DecodeOptions, DecodeStage, OrientationPolicy, RenditionData};
#[cfg(feature = "image")]
use crate::metadata::ExifOrientation;
use crate::model::Encoding;
use crate::model::rendition::{CompressionType, Rendition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaPayload {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[cfg(all(test, feature = "image", target_arch = "wasm32"))]
mod wasm_image_tests {
    use std::io::Cursor;

    use image::ImageDecoder;
    use wasm_bindgen_test::wasm_bindgen_test;

    use crate::Car;

    const ASSETS_CAR_BYTES: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../car-tests/data/Assets.car"
    ));

    #[wasm_bindgen_test]
    fn png_bytes_omit_host_icc_profiles_on_wasm() {
        let car = Car::from_bytes(ASSETS_CAR_BYTES.to_vec()).expect("load archive from bytes");
        let item = car
            .rendtions_with_name("2016_coin1")
            .and_then(|items| items.first())
            .expect("missing 2016_coin1 rendition");

        let png = item.png_bytes().expect("encode png bytes");
        let mut decoder =
            image::codecs::png::PngDecoder::new(Cursor::new(png)).expect("decode png bytes");

        assert!(
            decoder
                .icc_profile()
                .expect("reading ICC metadata should succeed")
                .is_none(),
            "wasm path should not read host ICC profile files"
        );
    }
}

#[cfg(feature = "image")]
fn resolve_bytes_per_row(item: &CSIItem, bpp: u32) -> CarResult<u32> {
    let min_row = item.header.width * bpp;
    if let Some(bytes_per_row) = item.bytes_per_row_tlv() {
        if bytes_per_row < min_row {
            return Err(CarError::DecodeFailed(
                "bytes_per_row smaller than minimum row size".to_string(),
            ));
        }
        return Ok(bytes_per_row);
    }
    Ok(min_row)
}

#[cfg(feature = "image")]
fn strip_stride(data: Vec<u8>, width: u32, height: u32, bpp: u32, stride: u32) -> Vec<u8> {
    let row_bytes = (width * bpp) as usize;
    let stride = stride as usize;
    if row_bytes == stride {
        return data;
    }
    let mut out = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        let start = row * stride;
        let end = start + row_bytes;
        if end <= data.len() {
            out.extend_from_slice(&data[start..end]);
        }
    }
    out
}

#[cfg(feature = "image")]
fn is_deepmap2(item: &CSIItem) -> bool {
    matches!(
        &item.header.rendition,
        Some(Rendition::ThemeCBCK(cbck)) if matches!(cbck.compression_type, CompressionType::Deepmap2)
    )
}

pub(crate) fn is_hevc_compressed(item: &CSIItem) -> bool {
    matches!(
        &item.header.rendition,
        Some(Rendition::ThemeCBCK(cbck)) if matches!(cbck.compression_type, CompressionType::HEVC)
    )
}

fn hevc_source_bytes(item: &CSIItem) -> CarResult<Option<Cow<'_, [u8]>>> {
    let Some(Rendition::ThemeCBCK(cbck)) = &item.header.rendition else {
        return Ok(None);
    };
    if !matches!(cbck.compression_type, CompressionType::HEVC) {
        return Ok(None);
    }
    if cbck.chunks.len() != 1 {
        return Err(CarError::DecodeFailed(format!(
            "HEVC raw export expects a single chunk, got {}",
            cbck.chunks.len()
        )));
    }

    heic_container_from_hevc_chunk(cbck.chunks[0].raw_data.as_slice())
        .map(|data| Some(Cow::Borrowed(data)))
}

fn heic_container_from_hevc_chunk(raw: &[u8]) -> CarResult<&[u8]> {
    const WRAPPER_LEN: usize = 8;
    const MIN_FTYP_LEN: usize = 16;

    if raw.len() < WRAPPER_LEN + MIN_FTYP_LEN {
        return Err(CarError::DecodeFailed(
            "HEVC chunk too short to contain HEIC payload".to_string(),
        ));
    }

    let declared_len = u32::from_le_bytes(raw[4..8].try_into().unwrap()) as usize;
    let body_len = raw.len() - WRAPPER_LEN;
    if declared_len != body_len {
        return Err(CarError::DecodeFailed(format!(
            "HEVC chunk length mismatch: declared {}, got {}",
            declared_len, body_len
        )));
    }

    let body = &raw[WRAPPER_LEN..];
    let ftyp_len = u32::from_be_bytes(body[0..4].try_into().unwrap()) as usize;
    if ftyp_len < MIN_FTYP_LEN || ftyp_len > body.len() {
        return Err(CarError::DecodeFailed(format!(
            "HEVC chunk has invalid ftyp box length: {ftyp_len}"
        )));
    }
    if &body[4..8] != b"ftyp" {
        return Err(CarError::DecodeFailed(
            "HEVC chunk does not start with HEIC ftyp box".to_string(),
        ));
    }
    if !ftyp_has_heif_brand(&body[..ftyp_len]) {
        return Err(CarError::DecodeFailed(
            "HEVC chunk ftyp box does not advertise HEIC/HEIF brand".to_string(),
        ));
    }

    Ok(body)
}

fn ftyp_has_heif_brand(ftyp_box: &[u8]) -> bool {
    is_heif_brand(&ftyp_box[8..12]) || ftyp_box[16..].chunks_exact(4).any(is_heif_brand)
}

fn is_heif_brand(brand: &[u8]) -> bool {
    matches!(
        brand,
        b"heic"
            | b"heix"
            | b"hevc"
            | b"hevx"
            | b"heim"
            | b"heis"
            | b"mif1"
            | b"msf1"
            | b"MiHE"
            | b"MiHB"
    )
}

#[cfg(feature = "image")]
fn unpremultiply_rgba8_in_place(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        let alpha = px[3];
        if alpha == 0 || alpha == u8::MAX {
            continue;
        }

        for channel in &mut px[..3] {
            let value = (((*channel as u32) * u8::MAX as u32) + (alpha as u32 / 2)) / alpha as u32;
            *channel = value.min(u8::MAX as u32) as u8;
        }
    }
}

#[cfg(feature = "image")]
fn unpremultiply_rgba16_in_place(pixels: &mut [u16]) {
    for px in pixels.chunks_exact_mut(4) {
        let alpha = px[3];
        if alpha == 0 || alpha == u16::MAX {
            continue;
        }

        for channel in &mut px[..3] {
            let value = (((*channel as u64) * u16::MAX as u64) + (alpha as u64 / 2)) / alpha as u64;
            *channel = value.min(u16::MAX as u64) as u16;
        }
    }
}

/// Crop `img` to the sub-region described by `rect`.
///
/// `RenditionTypeReference.(x, y, width, height)` stores a CGRect-style frame whose
/// origin is measured from the source image's bottom-left corner. `image` uses a
/// top-left origin, so `y` must be flipped before cropping.
#[cfg(feature = "image")]
fn apply_reference_frame(
    img: image::DynamicImage,
    rect: &super::ReferenceRect,
) -> CarResult<image::DynamicImage> {
    let img_width = img.width();
    let img_height = img.height();
    let right = rect.x.checked_add(rect.width);
    let top = rect.y.checked_add(rect.height);
    match (right, top) {
        (Some(right), Some(top)) if right <= img_width && top <= img_height => {
            let crop_y = img_height - top;
            Ok(img.crop_imm(rect.x, crop_y, rect.width, rect.height))
        }
        _ => Err(CarError::DecodeFailed(format!(
            "reference crop rect (x={}, y={}, w={}, h={}) exceeds source image {}x{}",
            rect.x, rect.y, rect.width, rect.height, img_width, img_height
        ))),
    }
}

#[cfg(feature = "image")]
#[cfg(not(target_arch = "wasm32"))]
fn png_icc_profile(item: &CSIItem) -> Option<Vec<u8>> {
    use crate::model::{ColorModel, rendition::AttributeType};
    use std::sync::OnceLock;

    static SRGB_PROFILE: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    static P3_PROFILE: OnceLock<Option<Vec<u8>>> = OnceLock::new();

    let is_display_p3 = item.header.color_model == ColorModel::RGBP3
        || item
            .attrs
            .iter()
            .any(|attr| attr.name == AttributeType::DisplayGamut && attr.val == 1);

    if is_display_p3 {
        P3_PROFILE
            .get_or_init(|| std::fs::read("/System/Library/ColorSync/Profiles/Display P3.icc").ok())
            .clone()
    } else {
        SRGB_PROFILE
            .get_or_init(|| {
                std::fs::read("/System/Library/ColorSync/Profiles/sRGB Profile.icc").ok()
            })
            .clone()
    }
}

#[cfg(feature = "image")]
#[cfg(target_arch = "wasm32")]
fn png_icc_profile(_item: &CSIItem) -> Option<Vec<u8>> {
    None
}

#[cfg(feature = "image")]
fn encode_png_with_icc<W>(item: &CSIItem, img: &image::DynamicImage, writer: W) -> CarResult<()>
where
    W: std::io::Write,
{
    use image::ImageEncoder;
    let mut encoder = image::codecs::png::PngEncoder::new(writer);
    if let Some(profile) = png_icc_profile(item) {
        let _ = encoder.set_icc_profile(profile);
    }
    encoder.write_image(
        img.as_bytes(),
        img.width(),
        img.height(),
        img.color().into(),
    )?;
    Ok(())
}

#[cfg(feature = "image")]
fn write_png_with_icc(item: &CSIItem, img: &image::DynamicImage, path: &Path) -> CarResult<()> {
    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    encode_png_with_icc(item, img, writer)
}

#[cfg(feature = "image")]
fn render_image_with_crops(
    item: &CSIItem,
    crops: &[super::ReferenceRect],
) -> CarResult<image::DynamicImage> {
    let mut img = item.to_image()?;
    for rect in crops {
        img = apply_reference_frame(img, rect)?;
    }
    Ok(img)
}

#[cfg(feature = "image")]
fn apply_exif_orientation(
    item: &CSIItem,
    options: &DecodeOptions,
    img: image::DynamicImage,
) -> image::DynamicImage {
    if !matches!(options.orientation_policy, OrientationPolicy::ApplyExif) {
        return img;
    }

    match item.exif_orientation() {
        Some(ExifOrientation::Mirrored) => img.fliph(),
        Some(ExifOrientation::Rotated180) => img.rotate180(),
        Some(ExifOrientation::Rotated180Mirrored) => img.rotate180().fliph(),
        Some(ExifOrientation::Rotated90) => img.rotate90(),
        Some(ExifOrientation::Rotated90Mirrored) => img.rotate90().fliph(),
        Some(ExifOrientation::Rotated270) => img.rotate270(),
        Some(ExifOrientation::Rotated270Mirrored) => img.rotate270().fliph(),
        Some(ExifOrientation::None | ExifOrientation::Normal | ExifOrientation::Unknown(_))
        | None => img,
    }
}

#[cfg(feature = "image")]
fn encode_png_bytes(item: &CSIItem, img: &image::DynamicImage) -> CarResult<Vec<u8>> {
    let mut buf = Vec::new();
    encode_png_with_icc(item, img, &mut buf)?;
    Ok(buf)
}

#[cfg(feature = "image")]
fn is_png_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
}

/// 热路径 PNG 直写：对 Deepmap2/ARGB/GA8/ARGB16 跳过 DynamicImage 中间层，
/// 将解码后的像素缓冲直接送入 PngEncoder，减少中间分配与多遍扫描。
///
/// 返回 `Ok(true)` 表示已处理，`Ok(false)` 表示当前编码不在直接路径上，
/// 调用方应回退到 `to_image()` + `save_image()` 通用路径。
#[cfg(feature = "image")]
fn save_image_to_png_direct(item: &CSIItem, path: &Path) -> CarResult<bool> {
    use image::{ExtendedColorType, ImageEncoder};

    // 提前检查编码是否在直写白名单中，避免对 GRAY/GA16/RGB5/JPEG/WEBP 等编码调用
    // decode_data()，随后又回退到 to_image() 导致整段数据被二次解码。
    let is_deepmap2 = is_deepmap2(item);
    if !is_deepmap2 {
        match item.header.encoding {
            Encoding::ARGB | Encoding::GA8 | Encoding::ARGB16 => {}
            _ => return Ok(false),
        }
    }

    let w = item.header.width;
    let h = item.header.height;

    let data = match item.decode_data()? {
        Some(RenditionData::Image { data }) => data,
        _ => return Ok(false),
    };

    // 每种编码产出 (像素字节, 颜色类型, 输出宽, 输出高)
    let (pixels, color_type, out_w, out_h): (Vec<u8>, ExtendedColorType, u32, u32) = if is_deepmap2
    {
        // Deepmap2 解码输出恒为 RGBA8888；仅在长度与期望不符时才做 stride 剥离
        let row_bytes = (w as usize).saturating_mul(4);
        if row_bytes == 0 {
            return Ok(false);
        }
        let mut pixels = if data.len() == row_bytes * h as usize {
            data
        } else {
            let stride = resolve_bytes_per_row(item, 4).unwrap_or(w * 4);
            strip_stride(data, w, h, 4, stride)
        };
        if pixels.len() % row_bytes != 0 {
            return Err(CarError::DecodeFailed(format!(
                "RGBA buffer size mismatch: expected a multiple of {row_bytes} bytes, got {}",
                pixels.len()
            )));
        }
        // CoreUI stores Deepmap2 RGBA payloads with premultiplied alpha on
        // semi-transparent edge pixels; convert to straight alpha before saving.
        unpremultiply_rgba8_in_place(&mut pixels);
        let inferred_h = (pixels.len() / row_bytes) as u32;
        (pixels, ExtendedColorType::Rgba8, w, inferred_h)
    } else {
        match item.header.encoding {
            Encoding::ARGB => {
                // 单遍：stride 剥离 + BGRA(预乘 alpha)→RGBA(直 alpha)
                let stride = resolve_bytes_per_row(item, 4)? as usize;
                let row_bytes = (w * 4) as usize;
                let mut pixels = Vec::with_capacity(row_bytes * h as usize);
                for row in 0..h as usize {
                    let start = row * stride;
                    let end = start + row_bytes;
                    if end > data.len() {
                        break;
                    }
                    for chunk in data[start..end].chunks_exact(4) {
                        pixels.push(chunk[2]); // R (BGRA→RGBA)
                        pixels.push(chunk[1]); // G
                        pixels.push(chunk[0]); // B
                        pixels.push(chunk[3]); // A
                    }
                }
                let expected = row_bytes * h as usize;
                if pixels.len() < expected {
                    return Err(CarError::DecodeFailed(format!(
                        "ARGB pixel buffer truncated: expected {expected} bytes, got {}",
                        pixels.len()
                    )));
                }
                unpremultiply_rgba8_in_place(&mut pixels);
                (pixels, ExtendedColorType::Rgba8, w, h)
            }
            Encoding::GA8 => {
                let stride = resolve_bytes_per_row(item, 2)?;
                let pixels = strip_stride(data, w, h, 2, stride);
                let expected = w as usize * h as usize * 2;
                if pixels.len() < expected {
                    return Err(CarError::DecodeFailed(format!(
                        "GA8 pixel buffer truncated: expected {expected} bytes, got {}",
                        pixels.len()
                    )));
                }
                (pixels, ExtendedColorType::La8, w, h)
            }
            Encoding::ARGB16 => {
                // 单遍：BGRA16LE(预乘 alpha)→RGBA16NE(直 alpha)
                // PngEncoder 的 Rgba16 颜色类型要求本机字节序 u16，不能直接透传 LE 字节。
                let stride = resolve_bytes_per_row(item, 8)? as usize;
                let row_bytes = (w * 8) as usize; // 4 通道 × 2 字节
                let mut pixels: Vec<u8> = Vec::with_capacity(w as usize * h as usize * 8);
                for row in 0..h as usize {
                    let row_start = row * stride;
                    if row_start + row_bytes > data.len() {
                        break;
                    }
                    for px in data[row_start..row_start + row_bytes].chunks_exact(8) {
                        // BGRA16LE → RGBA16NE，通道重排与反预乘在同一遍完成
                        let r = u16::from_le_bytes([px[4], px[5]]);
                        let g = u16::from_le_bytes([px[2], px[3]]);
                        let b = u16::from_le_bytes([px[0], px[1]]);
                        let a = u16::from_le_bytes([px[6], px[7]]);
                        let (r, g, b) = if a == 0 || a == u16::MAX {
                            (r, g, b)
                        } else {
                            let up = |c: u16| -> u16 {
                                ((c as u64 * u16::MAX as u64 + a as u64 / 2) / a as u64)
                                    .min(u16::MAX as u64) as u16
                            };
                            (up(r), up(g), up(b))
                        };
                        pixels.extend_from_slice(&r.to_ne_bytes());
                        pixels.extend_from_slice(&g.to_ne_bytes());
                        pixels.extend_from_slice(&b.to_ne_bytes());
                        pixels.extend_from_slice(&a.to_ne_bytes());
                    }
                }
                let expected = w as usize * h as usize * 8;
                if pixels.len() < expected {
                    return Err(CarError::DecodeFailed(format!(
                        "ARGB16 pixel buffer truncated: expected {expected} bytes, got {}",
                        pixels.len()
                    )));
                }
                (pixels, ExtendedColorType::Rgba16, w, h)
            }
            _ => return Ok(false),
        }
    };

    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = image::codecs::png::PngEncoder::new(writer);
    if let Some(profile) = png_icc_profile(item) {
        let _ = encoder.set_icc_profile(profile);
    }
    encoder.write_image(&pixels, out_w, out_h, color_type)?;
    Ok(true)
}

#[cfg(feature = "image")]
impl CSIItem {
    pub fn to_image(&self) -> CarResult<image::DynamicImage> {
        self.to_image_with_options(&DecodeOptions::default())
    }

    pub fn to_image_with_options(&self, options: &DecodeOptions) -> CarResult<image::DynamicImage> {
        let img = self.to_image_unoriented(options)?;
        options.check_output_bytes(img.as_bytes().len(), DecodeStage::ImageConversion)?;
        Ok(apply_exif_orientation(self, options, img))
    }

    fn to_image_unoriented(&self, options: &DecodeOptions) -> CarResult<image::DynamicImage> {
        use image::{DynamicImage, ImageBuffer};

        let data = match self.decode_data_with_options(options)? {
            Some(RenditionData::Image { data }) => data,
            _ => {
                return Err(CarError::UnsupportedEncoding(self.header.encoding));
            }
        };

        let w = self.header.width;
        let h = self.header.height;

        // Deepmap2 special case: always RGBA8888
        if is_deepmap2(self) {
            let row_bytes = (w as usize).saturating_mul(4);
            let header_len = row_bytes.saturating_mul(h as usize);
            let mut pixels = if data.len() == header_len {
                data
            } else {
                // Deepmap2 already returns a packed RGBA buffer. Some assets still carry a
                // BytesPerRow TLV for the original source pixel format, which is not applicable
                // after deepmap2 expansion. Only use stride stripping as a recovery path when the
                // decoded byte count does not already match the target RGBA image size.
                let stride = resolve_bytes_per_row(self, 4).unwrap_or(w * 4);
                strip_stride(data, w, h, 4, stride)
            };
            if row_bytes == 0 || pixels.len() % row_bytes != 0 {
                return Err(CarError::DecodeFailed(format!(
                    "RGBA buffer size mismatch: expected a multiple of {row_bytes} bytes, got {}",
                    pixels.len()
                )));
            }
            unpremultiply_rgba8_in_place(&mut pixels);
            let inferred_h = (pixels.len() / row_bytes) as u32;
            let buf = ImageBuffer::from_raw(w, inferred_h, pixels)
                .ok_or_else(|| CarError::DecodeFailed("RGBA buffer size mismatch".into()))?;
            return Ok(DynamicImage::ImageRgba8(buf));
        }

        match self.header.encoding {
            Encoding::ARGB => {
                let stride = resolve_bytes_per_row(self, 4)?;
                let mut pixels = strip_stride(data, w, h, 4, stride);
                // BGRA(预乘 alpha) → RGBA(直 alpha)
                for chunk in pixels.chunks_exact_mut(4) {
                    chunk.swap(0, 2);
                }
                unpremultiply_rgba8_in_place(&mut pixels);
                let buf = ImageBuffer::from_raw(w, h, pixels)
                    .ok_or_else(|| CarError::DecodeFailed("ARGB buffer size mismatch".into()))?;
                Ok(DynamicImage::ImageRgba8(buf))
            }
            Encoding::GRAY => {
                let stride = resolve_bytes_per_row(self, 1)?;
                let pixels = strip_stride(data, w, h, 1, stride);
                let buf = ImageBuffer::from_raw(w, h, pixels)
                    .ok_or_else(|| CarError::DecodeFailed("GRAY buffer size mismatch".into()))?;
                Ok(DynamicImage::ImageLuma8(buf))
            }
            Encoding::GA8 => {
                let stride = resolve_bytes_per_row(self, 2)?;
                let pixels = strip_stride(data, w, h, 2, stride);
                let buf = ImageBuffer::from_raw(w, h, pixels)
                    .ok_or_else(|| CarError::DecodeFailed("GA8 buffer size mismatch".into()))?;
                Ok(DynamicImage::ImageLumaA8(buf))
            }
            Encoding::ARGB16 => {
                let stride = resolve_bytes_per_row(self, 8)?;
                let stride = stride as usize;
                let row_bytes = (w * 8) as usize; // 4 channels × 2 bytes each (BGRA16)
                let mut pixels: Vec<u16> = Vec::with_capacity((w * h) as usize * 4);
                for row in 0..h as usize {
                    let row_start = row * stride;
                    if row_start + row_bytes > data.len() {
                        break;
                    }
                    // BGRA16 → RGBA16: write channels in R G B A order
                    for px in data[row_start..row_start + row_bytes].chunks_exact(8) {
                        let b = u16::from_le_bytes([px[0], px[1]]);
                        let g = u16::from_le_bytes([px[2], px[3]]);
                        let r = u16::from_le_bytes([px[4], px[5]]);
                        let a = u16::from_le_bytes([px[6], px[7]]);
                        pixels.push(r);
                        pixels.push(g);
                        pixels.push(b);
                        pixels.push(a);
                    }
                }
                unpremultiply_rgba16_in_place(&mut pixels);
                let buf = ImageBuffer::from_raw(w, h, pixels)
                    .ok_or_else(|| CarError::DecodeFailed("ARGB16 buffer size mismatch".into()))?;
                Ok(DynamicImage::ImageRgba16(buf))
            }
            Encoding::GA16 => {
                let stride = resolve_bytes_per_row(self, 4)?;
                let stride = stride as usize;
                let row_bytes = (w * 4) as usize; // 2 channels × 2 bytes each
                let mut pixels: Vec<u16> = Vec::with_capacity((w * h) as usize * 2);
                for row in 0..h as usize {
                    let row_start = row * stride;
                    if row_start + row_bytes > data.len() {
                        break;
                    }
                    for px in data[row_start..row_start + row_bytes].chunks_exact(4) {
                        pixels.push(u16::from_le_bytes([px[0], px[1]]));
                        pixels.push(u16::from_le_bytes([px[2], px[3]]));
                    }
                }
                let buf = ImageBuffer::from_raw(w, h, pixels)
                    .ok_or_else(|| CarError::DecodeFailed("GA16 buffer size mismatch".into()))?;
                Ok(DynamicImage::ImageLumaA16(buf))
            }
            Encoding::RGB5 => {
                let stride = resolve_bytes_per_row(self, 2)?;
                let stripped = strip_stride(data, w, h, 2, stride);
                // XRGB1555: bit layout [x r4 r3 r2 r1 r0 g4 g3 | g2 g1 g0 b4 b3 b2 b1 b0]
                let mut pixels = Vec::with_capacity((w * h * 3) as usize);
                for pair in stripped.chunks_exact(2) {
                    let val = u16::from_le_bytes([pair[0], pair[1]]);
                    let r5 = ((val >> 10) & 0x1F) as u8;
                    let g5 = ((val >> 5) & 0x1F) as u8;
                    let b5 = (val & 0x1F) as u8;
                    pixels.push((r5 << 3) | (r5 >> 2));
                    pixels.push((g5 << 3) | (g5 >> 2));
                    pixels.push((b5 << 3) | (b5 >> 2));
                }
                let buf = ImageBuffer::from_raw(w, h, pixels)
                    .ok_or_else(|| CarError::DecodeFailed("RGB5 buffer size mismatch".into()))?;
                Ok(DynamicImage::ImageRgb8(buf))
            }
            Encoding::JPEG | Encoding::WEBP => Ok(image::load_from_memory(&data)?),
            _ => Err(CarError::UnsupportedEncoding(self.header.encoding)),
        }
    }

    pub fn rgba_bytes(&self) -> CarResult<RgbaPayload> {
        self.rgba_bytes_with_crops(&[])
    }

    pub fn rgba_bytes_with_crops(&self, crops: &[super::ReferenceRect]) -> CarResult<RgbaPayload> {
        let img = render_image_with_crops(self, crops)?.to_rgba8();
        Ok(RgbaPayload {
            width: img.width(),
            height: img.height(),
            rgba: img.into_raw(),
        })
    }

    pub fn png_bytes(&self) -> CarResult<Vec<u8>> {
        self.png_bytes_with_crops(&[])
    }

    pub fn png_bytes_with_crops(&self, crops: &[super::ReferenceRect]) -> CarResult<Vec<u8>> {
        let img = render_image_with_crops(self, crops)?;
        encode_png_bytes(self, &img)
    }

    /// Decode the image, apply reference crop rectangles in order, then save to `path`.
    ///
    /// `crops` must be ordered **innermost-first** (as produced by
    /// `Car::resolve_internal_reference`).  Each rect is applied via
    /// `apply_reference_frame`: crop the current image at
    /// `(rect.x, rect.y, rect.width, rect.height)`.
    pub fn save_image_with_crops(
        &self,
        path: impl AsRef<Path>,
        crops: &[super::ReferenceRect],
    ) -> CarResult<()> {
        let path = path.as_ref();
        let img = render_image_with_crops(self, crops)?;

        if is_png_path(path) {
            write_png_with_icc(self, &img, path)?;
        } else {
            img.save(path)?;
        }

        Ok(())
    }

    pub fn save_image(&self, path: impl AsRef<Path>) -> CarResult<()> {
        let path = path.as_ref();

        // PNG 热路径：对已知像素格式直接编码，跳过 DynamicImage 中间层
        let is_png = is_png_path(path);
        if is_png && save_image_to_png_direct(self, path)? {
            return Ok(());
        }

        // 通用回退路径：其他格式或 PNG 中非热路径编码（GRAY/GA16/RGB5/JPEG/WEBP）
        let img = self.to_image()?;

        if is_png {
            write_png_with_icc(self, &img, path)?;
            return Ok(());
        }

        img.save(path)?;
        Ok(())
    }
}

impl CSIItem {
    pub fn source_bytes(&self) -> CarResult<Cow<'_, [u8]>> {
        use crate::decode::RenditionDataRef;

        if let Some(bytes) = hevc_source_bytes(self)? {
            return Ok(bytes);
        }

        let preserves_source = matches!(
            self.header.encoding,
            Encoding::Data
                | Encoding::HEIF
                | Encoding::PDF
                | Encoding::SVG
                | Encoding::JPEG
                | Encoding::WEBP
        );
        if !preserves_source {
            return Err(CarError::UnsupportedEncoding(self.header.encoding));
        }

        match &self.header.rendition {
            Some(Rendition::RawData(rd)) => {
                return util::apple_compression::maybe_decode_lzfse_with_fallback(
                    rd.raw_data.as_slice(),
                )
                .map_err(CarError::from);
            }
            Some(Rendition::ThemeCBCK(cbck))
                if matches!(cbck.compression_type, CompressionType::Uncompressed)
                    && matches!(
                        self.header.encoding,
                        Encoding::HEIF
                            | Encoding::PDF
                            | Encoding::SVG
                            | Encoding::JPEG
                            | Encoding::WEBP
                    ) =>
            {
                let mut bytes = Vec::new();
                for chunk in &cbck.chunks {
                    let data = util::apple_compression::maybe_decode_lzfse_with_fallback(
                        chunk.raw_data.as_slice(),
                    )?;
                    bytes.extend_from_slice(data.as_ref());
                }
                return Ok(Cow::Owned(bytes));
            }
            _ => {}
        }

        match self.decode_data_ref()? {
            Some(RenditionDataRef::RawData { data }) => Ok(data),
            Some(RenditionDataRef::Image { data }) => Ok(data),
            _ => Err(CarError::UnsupportedEncoding(self.header.encoding)),
        }
    }

    pub fn source_bytes_owned(&self) -> CarResult<Vec<u8>> {
        self.source_bytes().map(Cow::into_owned)
    }

    pub fn save_raw(&self, path: impl AsRef<Path>) -> CarResult<()> {
        let path = path.as_ref();
        let data = self.source_bytes()?;
        std::fs::write(path, data.as_ref())?;
        Ok(())
    }
}

#[cfg(test)]
mod test_helpers {
    use std::io::Cursor;

    use crate::car::CSIItem;
    use crate::model::CSIHeader;
    use deku::{DekuReader, reader::Reader};

    pub fn build_csi_bytes(
        width: u32,
        height: u32,
        encoding: &[u8; 4],
        tlv: &[u8],
        rendition: Option<&[u8]>,
    ) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"ISTC");
        b.extend_from_slice(&1u32.to_le_bytes()); // version
        b.extend_from_slice(&0u32.to_le_bytes()); // flags
        b.extend_from_slice(&width.to_le_bytes());
        b.extend_from_slice(&height.to_le_bytes());
        b.extend_from_slice(&100u32.to_le_bytes()); // scale_factor
        b.extend_from_slice(encoding);
        b.extend_from_slice(&0u32.to_le_bytes()); // color_model
        b.extend_from_slice(&0u32.to_le_bytes()); // modification_time
        b.extend_from_slice(&1000u32.to_le_bytes()); // layout_type
        b.extend_from_slice(&[0u8; 128]); // name
        b.extend_from_slice(&(tlv.len() as u32).to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes()); // bitmap_count
        b.extend_from_slice(&0u32.to_le_bytes()); // zero
        b.extend_from_slice(&(rendition.map_or(0, |d| d.len()) as u32).to_le_bytes());
        b.extend_from_slice(tlv);
        if let Some(r) = rendition {
            b.extend_from_slice(r);
        }
        b
    }

    pub fn parse_item(bytes: Vec<u8>) -> CSIItem {
        let mut reader = Reader::new(Cursor::new(bytes));
        let header =
            CSIHeader::from_reader_with_ctx(&mut reader, ()).expect("CSI header should parse");
        CSIItem {
            attrs: Vec::new(),
            header,
            key_values: Box::default(),
        }
    }

    pub fn uncompressed_rendition(data: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"MLEC");
        b.extend_from_slice(&0u32.to_le_bytes()); // version=0 single chunk
        b.extend_from_slice(&0u32.to_le_bytes()); // Uncompressed
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        b
    }

    pub fn hevc_rendition(heic_body: &[u8]) -> Vec<u8> {
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&1u32.to_le_bytes());
        chunk.extend_from_slice(&(heic_body.len() as u32).to_le_bytes());
        chunk.extend_from_slice(heic_body);

        let mut b = Vec::new();
        b.extend_from_slice(b"MLEC");
        b.extend_from_slice(&0u32.to_le_bytes()); // version=0 single chunk
        b.extend_from_slice(&9u32.to_le_bytes()); // HEVC
        b.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        b.extend_from_slice(&chunk);
        b
    }

    pub fn fake_heic_body() -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&24u32.to_be_bytes());
        body.extend_from_slice(b"ftyp");
        body.extend_from_slice(b"heic");
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(b"mif1");
        body.extend_from_slice(b"heic");
        body.extend_from_slice(&8u32.to_be_bytes());
        body.extend_from_slice(b"mdat");
        body
    }

    pub fn raw_data_rendition(data: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"DWAR");
        b.extend_from_slice(&1u32.to_le_bytes()); // version
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        b
    }

    pub fn bytes_per_row_tlv(bpr: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&1007u32.to_le_bytes()); // tag
        b.extend_from_slice(&4u32.to_le_bytes()); // length
        b.extend_from_slice(&bpr.to_le_bytes());
        b
    }

    pub fn exif_orientation_tlv(orientation: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&1006u32.to_le_bytes()); // tag
        b.extend_from_slice(&4u32.to_le_bytes()); // length
        b.extend_from_slice(&orientation.to_le_bytes());
        b
    }

    pub fn deepmap2_rendition(width: u16, height: u16, pixels: &[u8]) -> Vec<u8> {
        // Build dmp2 payload: "dmp2" + header + pixel data
        let mut dmp2 = Vec::new();
        dmp2.extend_from_slice(b"dmp2");
        dmp2.extend_from_slice(&[0x01, 0x00, 0x00, 0x01]); // version=1, compression=none, format=0, pixel_type=g8
        dmp2.extend_from_slice(&width.to_le_bytes());
        dmp2.extend_from_slice(&height.to_le_bytes());
        dmp2.extend_from_slice(pixels);
        // Wrap in ThemeCBCK with compression_type=11 (Deepmap2)
        let mut b = Vec::new();
        b.extend_from_slice(b"MLEC");
        b.extend_from_slice(&0u32.to_le_bytes()); // version=0 single chunk
        b.extend_from_slice(&11u32.to_le_bytes()); // Deepmap2
        b.extend_from_slice(&(dmp2.len() as u32).to_le_bytes());
        b.extend_from_slice(&dmp2);
        b
    }

    pub fn deepmap2_rendition_ga88(width: u16, height: u16, pixels: &[u8]) -> Vec<u8> {
        let mut dmp2 = Vec::new();
        dmp2.extend_from_slice(b"dmp2");
        dmp2.extend_from_slice(&[0x01, 0x00, 0x00, 0x02]); // pixel_type=ga88
        dmp2.extend_from_slice(&width.to_le_bytes());
        dmp2.extend_from_slice(&height.to_le_bytes());
        dmp2.extend_from_slice(pixels);

        let mut b = Vec::new();
        b.extend_from_slice(b"MLEC");
        b.extend_from_slice(&0u32.to_le_bytes()); // version=0 single chunk
        b.extend_from_slice(&11u32.to_le_bytes()); // Deepmap2
        b.extend_from_slice(&(dmp2.len() as u32).to_le_bytes());
        b.extend_from_slice(&dmp2);
        b
    }

    pub fn make_item(width: u32, height: u32, enc: &[u8; 4], pixels: &[u8]) -> CSIItem {
        let r = uncompressed_rendition(pixels);
        parse_item(build_csi_bytes(width, height, enc, &[], Some(&r)))
    }

    pub fn make_item_stride(
        width: u32,
        height: u32,
        enc: &[u8; 4],
        pixels: &[u8],
        bpr: u32,
    ) -> CSIItem {
        let r = uncompressed_rendition(pixels);
        let tlv = bytes_per_row_tlv(bpr);
        parse_item(build_csi_bytes(width, height, enc, &tlv, Some(&r)))
    }
}

#[cfg(test)]
#[cfg(feature = "image")]
mod tests {
    use std::io::Cursor;

    use super::test_helpers::*;
    use crate::DecodeOptions;
    use crate::car::CarError;

    // ── ARGB (BGRA → RGBA swizzle) ─────────────────────────────────

    #[test]
    fn to_image_argb_swizzle() {
        // 2×1 BGRA pixels; second pixel uses premultiplied alpha:
        // straight RGBA [255, 128, 64, 128] -> stored BGRA [32, 64, 128, 128]
        let pixels = [10u8, 20, 30, 255, 32, 64, 128, 128];
        let item = make_item(2, 1, b"BGRA", &pixels);
        let img = item.to_image().unwrap();
        let rgba = img.to_rgba8();
        assert_eq!(rgba.get_pixel(0, 0).0, [30, 20, 10, 255]);
        assert_eq!(rgba.get_pixel(1, 0).0, [255, 128, 64, 128]);
    }

    // ── ARGB16 (BGRA16 → RGBA16, little-endian) ────────────────────

    #[test]
    fn to_image_argb16_swizzle_le() {
        // 1×1 BGRA16 (premultiplied alpha):
        // straight RGBA16 [65535, 32768, 16384, 32768]
        // -> stored BGRA16 [8192, 16384, 32768, 32768]
        let pixels = [
            0x00u8, 0x20, // B=8192
            0x00, 0x40, // G=16384
            0x00, 0x80, // R=32768
            0x00, 0x80, // A=32768
        ];
        let item = make_item(1, 1, b"WBGR", &pixels);
        let img = item.to_image().unwrap();
        let rgba16 = img.to_rgba16();
        assert_eq!(rgba16.get_pixel(0, 0).0, [65535, 32768, 16384, 32768]);
    }

    // ── GRAY ────────────────────────────────────────────────────────

    #[test]
    fn to_image_gray() {
        let pixels = [100u8, 200];
        let item = make_item(2, 1, b"YARG", &pixels);
        let img = item.to_image().unwrap();
        let luma = img.to_luma8();
        assert_eq!(luma.get_pixel(0, 0).0, [100]);
        assert_eq!(luma.get_pixel(1, 0).0, [200]);
    }

    // ── GA8 ─────────────────────────────────────────────────────────

    #[test]
    fn to_image_ga8() {
        let pixels = [100u8, 200]; // L=100, A=200
        let item = make_item(1, 1, b" 8AG", &pixels);
        let img = item.to_image().unwrap();
        let la = img.to_luma_alpha8();
        assert_eq!(la.get_pixel(0, 0).0, [100, 200]);
    }

    // ── GA16 (little-endian) ────────────────────────────────────────

    #[test]
    fn to_image_ga16_le() {
        // L=0x1234, A=0x5678 in LE
        let pixels = [0x34u8, 0x12, 0x78, 0x56];
        let item = make_item(1, 1, b"61AG", &pixels);
        let img = item.to_image().unwrap();
        let la16 = img.to_luma_alpha16();
        assert_eq!(la16.get_pixel(0, 0).0, [0x1234, 0x5678]);
    }

    // ── RGB5 (XRGB1555) ────────────────────────────────────────────

    #[test]
    fn to_image_rgb5() {
        // R=31 G=0 B=0 → 0b0_11111_00000_00000 = 0x7C00
        let pixels = 0x7C00u16.to_le_bytes();
        let item = make_item(1, 1, b"5BGR", &pixels);
        let img = item.to_image().unwrap();
        let rgb = img.to_rgb8();
        // R: (31<<3)|(31>>2) = 255, G: 0, B: 0
        assert_eq!(rgb.get_pixel(0, 0).0, [255, 0, 0]);
    }

    // ── Deepmap2 special case ───────────────────────────────────────

    #[test]
    fn to_image_deepmap2_rgba() {
        // 2×1 Deepmap2 with grayscale pixels [100, 200]
        // Deepmap2 always produces RGBA: [v, v, v, 0xFF] per pixel
        let r = deepmap2_rendition(2, 1, &[100, 200]);
        // Use GRAY encoding to prove Deepmap2 overrides it
        let item = parse_item(build_csi_bytes(2, 1, b"YARG", &[], Some(&r)));
        let img = item.to_image().unwrap();
        let rgba = img.to_rgba8();
        assert_eq!(rgba.get_pixel(0, 0).0, [100, 100, 100, 255]);
        assert_eq!(rgba.get_pixel(1, 0).0, [200, 200, 200, 255]);
    }

    #[test]
    fn to_image_deepmap2_ignores_incompatible_source_stride() {
        let r = deepmap2_rendition_ga88(2, 1, &[100, 255, 200, 255]);
        let tlv = bytes_per_row_tlv(2);
        let item = parse_item(build_csi_bytes(2, 1, b" 8AG", &tlv, Some(&r)));
        let img = item.to_image().unwrap();
        let rgba = img.to_rgba8();
        assert_eq!(rgba.get_pixel(0, 0).0, [100, 100, 100, 255]);
        assert_eq!(rgba.get_pixel(1, 0).0, [200, 200, 200, 255]);
    }

    #[test]
    fn to_image_deepmap2_uses_decoded_height_when_header_mismatches() {
        let r = deepmap2_rendition(2, 1, &[100, 200]);
        let item = parse_item(build_csi_bytes(2, 2, b"YARG", &[], Some(&r)));
        let img = item.to_image().unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 1);
    }

    // ── EXIF orientation ────────────────────────────────────────────

    #[test]
    fn to_image_exif_orientation_ignored() {
        // 2×1 GRAY with EXIF orientation TLV (Rotate90CW=6) present
        let pixels = [100u8, 200];
        let tlv = exif_orientation_tlv(6);
        let r = uncompressed_rendition(&pixels);
        let item = parse_item(build_csi_bytes(2, 1, b"YARG", &tlv, Some(&r)));
        let img = item.to_image().unwrap();
        let luma = img.to_luma8();
        // Pixels unchanged — no rotation applied
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 1);
        assert_eq!(luma.get_pixel(0, 0).0, [100]);
        assert_eq!(luma.get_pixel(1, 0).0, [200]);
    }

    #[test]
    fn to_image_exif_orientation_can_be_applied() {
        let pixels = [
            1u8, 2, // row 0
            3, 4, // row 1
        ];
        let tlv = exif_orientation_tlv(6);
        let r = uncompressed_rendition(&pixels);
        let item = parse_item(build_csi_bytes(2, 2, b"YARG", &tlv, Some(&r)));

        let img = item
            .to_image_with_options(&DecodeOptions::default().apply_exif_orientation(true))
            .unwrap();
        let luma = img.to_luma8();

        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
        assert_eq!(luma.get_pixel(0, 0).0, [3]);
        assert_eq!(luma.get_pixel(1, 0).0, [1]);
        assert_eq!(luma.get_pixel(0, 1).0, [4]);
        assert_eq!(luma.get_pixel(1, 1).0, [2]);
    }

    // ── stride padding ──────────────────────────────────────────────

    #[test]
    fn to_image_stride_strips_padding() {
        // 1×2 GRAY, min row = 1 byte, stride = 4 (3 bytes padding per row)
        let pixels = [100u8, 0, 0, 0, 200, 0, 0, 0];
        let item = make_item_stride(1, 2, b"YARG", &pixels, 4);
        let img = item.to_image().unwrap();
        let luma = img.to_luma8();
        assert_eq!(luma.get_pixel(0, 0).0, [100]);
        assert_eq!(luma.get_pixel(0, 1).0, [200]);
    }

    #[test]
    fn to_image_invalid_bytes_per_row() {
        // 2×1 ARGB: min row = 8 bytes, but BytesPerRow = 4 → error
        let pixels = [0u8; 8];
        let item = make_item_stride(2, 1, b"BGRA", &pixels, 4);
        let err = item.to_image().unwrap_err();
        assert!(matches!(err, CarError::DecodeFailed(_)));
        assert!(format!("{err}").contains("bytes_per_row"));
    }

    // ── JPEG ────────────────────────────────────────────────────────

    #[test]
    fn to_image_jpeg() {
        use image::{ImageBuffer, ImageFormat, RgbImage};
        // Create a minimal 1×1 JPEG in memory
        let rgb: RgbImage = ImageBuffer::from_raw(1, 1, vec![255u8, 0, 0]).unwrap();
        let mut buf = Cursor::new(Vec::new());
        rgb.write_to(&mut buf, ImageFormat::Jpeg).unwrap();
        let jpeg_bytes = buf.into_inner();

        let item = make_item(1, 1, b"GEPJ", &jpeg_bytes);
        let img = item.to_image().unwrap();
        assert_eq!(img.width(), 1);
        assert_eq!(img.height(), 1);
    }

    // ── unsupported encoding ────────────────────────────────────────

    #[test]
    fn to_image_unsupported_encoding() {
        let item = make_item(1, 1, b"ATAD", &[0u8; 4]);
        let err = item.to_image().unwrap_err();
        assert!(matches!(err, CarError::UnsupportedEncoding(_)));
    }

    // ── save_image ──────────────────────────────────────────────────

    #[test]
    fn save_image_png() {
        let pixels = [10u8, 20, 30, 255]; // 1×1 BGRA pixel
        let item = make_item(1, 1, b"BGRA", &pixels);
        let dir = std::env::temp_dir().join("carparser_test_img");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_save.png");
        item.save_image(&path).unwrap();
        let img = image::open(&path).unwrap();
        assert_eq!(img.width(), 1);
        assert_eq!(img.height(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_image_unsupported_format() {
        let pixels = [10u8, 20, 30, 255]; // 1×1 BGRA pixel
        let item = make_item(1, 1, b"BGRA", &pixels);
        let dir = std::env::temp_dir().join("carparser_test_img");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_save.bmp");
        let err = item.save_image(&path).unwrap_err();
        assert!(matches!(err, CarError::Image(_)));
    }

    // ── 直接 PNG 编码路径 ──────────────────────────────────────────────

    #[test]
    fn save_image_png_direct_argb_pixel_correct() {
        // 1×1 BGRA: B=10 G=20 R=30 A=255 → PNG RGBA: R=30 G=20 B=10 A=255
        let pixels = [10u8, 20, 30, 255];
        let item = make_item(1, 1, b"BGRA", &pixels);
        let dir = std::env::temp_dir().join("carparser_direct_argb");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("argb.png");
        item.save_image(&path).unwrap();
        let img = image::open(&path).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0, [30, 20, 10, 255]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_image_png_direct_argb_with_stride() {
        // 1×2 BGRA，stride=8（每行 4 字节有效 + 4 字节填充）
        // Row0: [B=1 G=2 R=3 A=255 PAD PAD PAD PAD]
        // Row1 stores premultiplied alpha for straight RGBA [12, 10, 8, 128]:
        // [B=4 G=5 R=6 A=128 PAD PAD PAD PAD]
        let pixels = [1u8, 2, 3, 255, 0, 0, 0, 0, 4, 5, 6, 128, 0, 0, 0, 0];
        let item = make_item_stride(1, 2, b"BGRA", &pixels, 8);
        let dir = std::env::temp_dir().join("carparser_direct_argb_stride");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("argb_stride.png");
        item.save_image(&path).unwrap();
        let img = image::open(&path).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (1, 2));
        assert_eq!(img.get_pixel(0, 0).0, [3, 2, 1, 255]);
        assert_eq!(img.get_pixel(0, 1).0, [12, 10, 8, 128]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_image_png_direct_ga8_pixel_correct() {
        // 1×1 GA8: L=128 A=200
        let pixels = [128u8, 200];
        let item = make_item(1, 1, b" 8AG", &pixels);
        let dir = std::env::temp_dir().join("carparser_direct_ga8");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("ga8.png");
        item.save_image(&path).unwrap();
        let img = image::open(&path).unwrap().to_luma_alpha8();
        assert_eq!(img.get_pixel(0, 0).0, [128, 200]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_image_png_direct_argb16_pixel_correct() {
        // 1×1 BGRA16 LE (premultiplied alpha):
        // straight RGBA16 [65535, 32768, 16384, 32768]
        // -> stored BGRA16 [8192, 16384, 32768, 32768]
        let pixels = [
            0x00u8, 0x20, // B=8192
            0x00, 0x40, // G=16384
            0x00, 0x80, // R=32768
            0x00, 0x80, // A=32768
        ];
        let item = make_item(1, 1, b"WBGR", &pixels);
        let dir = std::env::temp_dir().join("carparser_direct_argb16");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("argb16.png");
        item.save_image(&path).unwrap();
        let img = image::open(&path).unwrap().to_rgba16();
        assert_eq!(img.get_pixel(0, 0).0, [65535, 32768, 16384, 32768]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_image_png_direct_deepmap2_pixel_correct() {
        // 2×1 Deepmap2 灰度 → RGBA [v v v 255] per pixel
        let r = deepmap2_rendition(2, 1, &[100, 200]);
        let item = parse_item(build_csi_bytes(2, 1, b"YARG", &[], Some(&r)));
        let dir = std::env::temp_dir().join("carparser_direct_dmp2");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("deepmap2.png");
        item.save_image(&path).unwrap();
        let img = image::open(&path).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0, [100, 100, 100, 255]);
        assert_eq!(img.get_pixel(1, 0).0, [200, 200, 200, 255]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rgba_bytes_matches_rendered_rgba_output() {
        let pixels = [10u8, 20, 30, 255];
        let item = make_item(1, 1, b"BGRA", &pixels);

        let rendered = item.to_image().unwrap().to_rgba8();
        let rgba = item.rgba_bytes().unwrap();

        assert_eq!(rgba.width, 1);
        assert_eq!(rgba.height, 1);
        assert_eq!(rgba.rgba, rendered.into_raw());
    }

    #[test]
    fn png_bytes_encodes_png_in_memory() {
        let pixels = [10u8, 20, 30, 255];
        let item = make_item(1, 1, b"BGRA", &pixels);

        let png = item.png_bytes().unwrap();
        let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .unwrap()
            .to_rgba8();

        assert_eq!(decoded.dimensions(), (1, 1));
        assert_eq!(decoded.get_pixel(0, 0).0, [30, 20, 10, 255]);
    }
}

#[cfg(test)]
mod save_raw_tests {
    use super::test_helpers::*;
    use crate::Encoding;
    use crate::car::{Car, CarError};
    use crate::model::rendition::Rendition;
    use test_support::fixture_group;

    #[test]
    fn save_raw_with_raw_data() {
        let payload = b"hello-raw-data";
        let r = raw_data_rendition(payload);
        let item = parse_item(build_csi_bytes(1, 1, b"ATAD", &[], Some(&r)));
        let dir = std::env::temp_dir().join("carparser_test_raw");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.bin");
        item.save_raw(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), payload);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn source_bytes_with_raw_data() {
        let payload = b"hello-raw-data";
        let r = raw_data_rendition(payload);
        let item = parse_item(build_csi_bytes(1, 1, b"ATAD", &[], Some(&r)));

        let bytes = item.source_bytes().unwrap();
        assert_eq!(bytes.as_ref(), payload);
    }

    #[test]
    fn save_raw_heif_encoding() {
        let payload = b"fake-heif-data";
        let item = make_item(1, 1, b"FIEH", payload);
        let dir = std::env::temp_dir().join("carparser_test_raw");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.heif");
        item.save_raw(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), payload);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn source_bytes_hevc_cbck_strips_coreui_wrapper() {
        let heic = fake_heic_body();
        let rendition = hevc_rendition(&heic);
        let item = parse_item(build_csi_bytes(20, 88, b"BGRA", &[], Some(&rendition)));

        let bytes = item.source_bytes().unwrap();
        assert_eq!(bytes.as_ref(), heic.as_slice());
    }

    #[test]
    fn save_raw_hevc_cbck_writes_heic_container() {
        let heic = fake_heic_body();
        let rendition = hevc_rendition(&heic);
        let item = parse_item(build_csi_bytes(20, 88, b"BGRA", &[], Some(&rendition)));
        let dir = std::env::temp_dir().join("carparser_test_raw");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hevc.heic");

        item.save_raw(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), heic);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_raw_pdf_encoding() {
        let payload = b"fake-pdf-data";
        let item = make_item(1, 1, b" FDP", payload);
        let dir = std::env::temp_dir().join("carparser_test_raw");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.pdf");
        item.save_raw(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), payload);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_raw_svg_encoding() {
        let payload = b"<svg></svg>";
        let item = make_item(1, 1, b" GVS", payload);
        let dir = std::env::temp_dir().join("carparser_test_raw");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.svg");
        item.save_raw(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), payload);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn source_bytes_preserves_svg_payload_in_memory() {
        let payload = b"<svg></svg>";
        let item = make_item(1, 1, b" GVS", payload);

        let bytes = item.source_bytes().unwrap();
        assert_eq!(bytes.as_ref(), payload);
    }

    #[test]
    fn save_raw_rejects_pixel_format_encoding() {
        let item = make_item(1, 1, b"BGRA", &[0u8; 4]);
        let dir = std::env::temp_dir().join("carparser_test_raw");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_reject.bin");
        let err = item.save_raw(&path).unwrap_err();
        assert!(matches!(err, CarError::UnsupportedEncoding(_)));
    }

    // ── 直接写路径：ThemeCBCK Uncompressed HEIF/PDF/SVG ───────────────

    #[test]
    fn save_raw_heif_uncompressed_cbck_direct() {
        // HEIF 存储为 ThemeCBCK Uncompressed（make_item 构造的正是这种结构）
        // 验证直接写路径产出与 payload 完全一致
        let payload = b"heif-cbck-direct-write";
        let item = make_item(1, 1, b"FIEH", payload);
        let dir = std::env::temp_dir().join("carparser_raw_direct");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("heif_direct.heic");
        item.save_raw(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), payload);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_raw_pdf_uncompressed_cbck_direct() {
        let payload = b"%PDF-1.4 direct-write";
        let item = make_item(1, 1, b" FDP", payload);
        let dir = std::env::temp_dir().join("carparser_raw_direct");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("pdf_direct.pdf");
        item.save_raw(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), payload);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_raw_rawdata_direct_write() {
        // RawData 渲染类型直写：不经 decode_data() clone
        let payload = b"rawdata-no-clone-path";
        let r = raw_data_rendition(payload);
        let item = parse_item(build_csi_bytes(1, 1, b"ATAD", &[], Some(&r)));
        let dir = std::env::temp_dir().join("carparser_raw_direct");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("rawdata_direct.bin");
        item.save_raw(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), payload);
        let _ = std::fs::remove_file(&path);
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
    fn save_raw_assets_ios_svg_rawdata_decodes_apple_compression_stream() {
        let Some(car) = fixture_ios_car() else {
            return;
        };
        let item = car
            .rendtions_with_name("icon_device_migration")
            .and_then(|items| {
                items.iter().find(|item| {
                    matches!(item.header.rendition, Some(Rendition::RawData(_)))
                        && matches!(item.header.encoding, Encoding::SVG)
                })
            })
            .expect("missing icon_device_migration SVG RawData rendition");

        let dir = std::env::temp_dir().join("carparser_raw_fixture");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("icon_device_migration.svg");

        item.save_raw(&path).unwrap();
        let saved = std::fs::read(&path).unwrap();

        assert!(
            saved.starts_with(b"<?xml"),
            "expected decoded SVG XML, got magic {:?}",
            saved.get(..4)
        );
        assert!(saved.windows(b"<svg".len()).any(|w| w == b"<svg"));
        assert!(
            !saved.starts_with(b"bvx"),
            "saved file should not keep Apple compression stream magic"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// Verify that `apply_reference_frame` crops a sub-region from the source atlas.
    ///
    /// Atlas layout (12×4):
    ///   cols 0-5, rows 0-1: red
    ///   cols 6-11, rows 0-1: blue
    ///   all cols,  rows 2-3: green
    ///
    /// Crop rect (x=6, y=2, w=6, h=2) uses a bottom-left origin and should
    /// therefore extract the blue top-right region.
    #[cfg(feature = "image")]
    #[test]
    fn apply_reference_frame_crops_atlas_subregion() {
        use super::apply_reference_frame;
        use crate::car::ReferenceRect;

        let source = image::RgbaImage::from_fn(12, 4, |x, y| {
            if y < 2 {
                if x < 6 {
                    image::Rgba([255u8, 0, 0, 255]) // red
                } else {
                    image::Rgba([0u8, 0, 255, 255]) // blue
                }
            } else {
                image::Rgba([0u8, 255, 0, 255]) // green
            }
        });
        let img = image::DynamicImage::ImageRgba8(source);

        let rect = ReferenceRect {
            x: 6,
            y: 2,
            width: 6,
            height: 2,
        };
        let result = apply_reference_frame(img, &rect).unwrap();

        assert_eq!(result.width(), 6);
        assert_eq!(result.height(), 2);
        let rgba = result.to_rgba8();
        // Every pixel in the cropped result must be blue
        assert_eq!(
            rgba.get_pixel(0, 0).0,
            [0, 0, 255, 255],
            "top-left should be blue"
        );
        assert_eq!(
            rgba.get_pixel(5, 1).0,
            [0, 0, 255, 255],
            "bottom-right should be blue"
        );
        // No red or green pixels leaked in
        assert_ne!(rgba.get_pixel(0, 0).0[0], 255, "no red channel leak");
    }

    /// Out-of-bounds crop rects must surface as `CarError::DecodeFailed` rather than
    /// triggering an unsigned underflow / debug panic inside `crop_imm`.
    #[cfg(feature = "image")]
    #[test]
    fn apply_reference_frame_rejects_out_of_bounds_rect() {
        use super::apply_reference_frame;
        use crate::car::ReferenceRect;

        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(4, 4));

        let rect = ReferenceRect {
            x: 0,
            y: 2,
            width: 4,
            height: 8,
        };
        let err = apply_reference_frame(img, &rect).unwrap_err();
        assert!(matches!(err, CarError::DecodeFailed(_)));
        assert!(format!("{err}").contains("exceeds source image"));
    }
}
