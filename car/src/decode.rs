use std::{borrow::Cow, io::Read};

use flate2::read::MultiGzDecoder;
use lzfse_rust::LzfseRingDecoder;
use thiserror::Error;

use crate::car::{CarError, CarResult};
use crate::model::rendition::{
    CompressionType, MultisizeImageSetEntry, Rendition, RenditionThemeCBCK,
};
use crate::model::{ColorSpace, Encoding};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrientationPolicy {
    Ignore,
    ApplyExif,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeOptions {
    pub max_output_bytes: Option<usize>,
    pub orientation_policy: OrientationPolicy,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            max_output_bytes: None,
            orientation_policy: OrientationPolicy::Ignore,
        }
    }
}

impl DecodeOptions {
    pub fn with_max_output_bytes(mut self, max_output_bytes: usize) -> Self {
        self.max_output_bytes = Some(max_output_bytes);
        self
    }

    pub fn apply_exif_orientation(mut self, apply: bool) -> Self {
        self.orientation_policy = if apply {
            OrientationPolicy::ApplyExif
        } else {
            OrientationPolicy::Ignore
        };
        self
    }

    pub(crate) fn check_output_bytes(&self, requested: usize, stage: DecodeStage) -> CarResult<()> {
        if let Some(limit) = self.max_output_bytes
            && requested > limit
        {
            return Err(CarError::DecodeBudgetExceeded(
                DecodeBudgetError::OutputBytesExceeded {
                    limit,
                    requested,
                    stage,
                },
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeStage {
    BorrowedPayload,
    Uncompressed,
    Lzfse,
    Zip,
    Lzvn,
    Deepmap2,
    ImageConversion,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeBudgetError {
    #[error(
        "decoded output would exceed budget at {stage:?}: requested {requested}, limit {limit}"
    )]
    OutputBytesExceeded {
        limit: usize,
        requested: usize,
        stage: DecodeStage,
    },
}

#[derive(Debug)]
pub enum RenditionData {
    Color {
        color_space: ColorSpace,
        components: Vec<f64>,
    },
    Image {
        data: Vec<u8>,
    },
    RawData {
        data: Vec<u8>,
    },
    MultisizeImageSet {
        entries: Vec<MultisizeImageSetEntry>,
    },
}

#[derive(Debug)]
pub enum RenditionDataRef<'a> {
    Color {
        color_space: Cow<'a, ColorSpace>,
        components: Cow<'a, [f64]>,
    },
    Image {
        data: Cow<'a, [u8]>,
    },
    RawData {
        data: Cow<'a, [u8]>,
    },
    MultisizeImageSet {
        entries: Cow<'a, [MultisizeImageSetEntry]>,
    },
}

impl<'a> RenditionDataRef<'a> {
    pub fn into_owned(self) -> RenditionData {
        match self {
            Self::Color {
                color_space,
                components,
            } => RenditionData::Color {
                color_space: color_space.into_owned(),
                components: components.into_owned(),
            },
            Self::Image { data } => RenditionData::Image {
                data: data.into_owned(),
            },
            Self::RawData { data } => RenditionData::RawData {
                data: data.into_owned(),
            },
            Self::MultisizeImageSet { entries } => RenditionData::MultisizeImageSet {
                entries: entries.into_owned(),
            },
        }
    }

    fn from_owned(data: RenditionData) -> Self {
        match data {
            RenditionData::Color {
                color_space,
                components,
            } => Self::Color {
                color_space: Cow::Owned(color_space),
                components: Cow::Owned(components),
            },
            RenditionData::Image { data } => Self::Image {
                data: Cow::Owned(data),
            },
            RenditionData::RawData { data } => Self::RawData {
                data: Cow::Owned(data),
            },
            RenditionData::MultisizeImageSet { entries } => Self::MultisizeImageSet {
                entries: Cow::Owned(entries),
            },
        }
    }
}

fn check_borrowed_payload_bytes<T>(options: &DecodeOptions, payload: &[T]) -> CarResult<()> {
    options.check_output_bytes(std::mem::size_of_val(payload), DecodeStage::BorrowedPayload)
}

impl crate::car::CSIItem {
    pub fn decode_data_ref(&self) -> CarResult<Option<RenditionDataRef<'_>>> {
        self.decode_data_ref_with_options(&DecodeOptions::default())
    }

    pub fn decode_data_ref_with_options(
        &self,
        options: &DecodeOptions,
    ) -> CarResult<Option<RenditionDataRef<'_>>> {
        let rendition = match &self.header.rendition {
            None => return Ok(None),
            Some(r) => r,
        };

        match rendition {
            Rendition::Color(rc) => {
                check_borrowed_payload_bytes(options, rc.components.as_slice())?;
                Ok(Some(RenditionDataRef::Color {
                    color_space: Cow::Borrowed(&rc.color_space),
                    components: Cow::Borrowed(&rc.components),
                }))
            }
            Rendition::RawData(rd) => {
                check_borrowed_payload_bytes(options, rd.raw_data.as_slice())?;
                Ok(Some(RenditionDataRef::RawData {
                    data: Cow::Borrowed(rd.raw_data.as_slice()),
                }))
            }
            Rendition::MultisizeImageSet(mis) => {
                check_borrowed_payload_bytes(options, mis.entries.as_slice())?;
                Ok(Some(RenditionDataRef::MultisizeImageSet {
                    entries: Cow::Borrowed(mis.entries.as_slice()),
                }))
            }
            Rendition::ThemeCBCK(cbck) => decode_theme_cbck(self, cbck, options)
                .map(|data| data.map(RenditionDataRef::from_owned)),
            Rendition::Unknown { tag, .. } => Err(CarError::DecodeFailed(format!(
                "Unknown rendition type: {:?}",
                tag
            ))),
        }
    }

    pub fn decode_data_owned(&self) -> CarResult<Option<RenditionData>> {
        self.decode_data_ref_with_options(&DecodeOptions::default())
            .map(|data| data.map(RenditionDataRef::into_owned))
    }

    pub fn decode_data_owned_with_options(
        &self,
        options: &DecodeOptions,
    ) -> CarResult<Option<RenditionData>> {
        self.decode_data_ref_with_options(options)
            .map(|data| data.map(RenditionDataRef::into_owned))
    }

    pub fn decode_data(&self) -> CarResult<Option<RenditionData>> {
        self.decode_data_owned()
    }

    pub fn decode_data_with_options(
        &self,
        options: &DecodeOptions,
    ) -> CarResult<Option<RenditionData>> {
        self.decode_data_owned_with_options(options)
    }
}

fn deepmap2_pixel_format_override(encoding: Encoding) -> Option<deepmap2::PixelFormat> {
    match encoding {
        Encoding::ARGB => Some(deepmap2::PixelFormat::Rgba8888),
        Encoding::ARGB16 => Some(deepmap2::PixelFormat::Rgba8888),
        Encoding::GA8 => Some(deepmap2::PixelFormat::GA88),
        _ => None,
    }
}

/// 像素编码的每像素字节数（仅对具有固定 pixel stride 的 raster encoding 返回 Some）
fn encoding_bytes_per_pixel(encoding: Encoding) -> Option<usize> {
    match encoding {
        Encoding::ARGB => Some(4),
        Encoding::GRAY => Some(1),
        Encoding::GA8 => Some(2),
        Encoding::ARGB16 => Some(8),
        Encoding::GA16 => Some(4),
        Encoding::RGB5 => Some(2),
        _ => None,
    }
}

fn chunk_output_height(
    chunk: &crate::model::rendition::ThemeChunk,
    total_chunks: usize,
    full_height: u16,
) -> CarResult<Option<u16>> {
    match chunk.part_header.as_ref().map(|header| header.height) {
        Some(height) if total_chunks == 1 && height == 0 => Ok(Some(full_height)),
        None if total_chunks == 1 => Ok(Some(full_height)),
        Some(0) | None => Ok(None),
        Some(height) => u16::try_from(height).map(Some).map_err(|_| {
            CarError::DecodeFailed(format!("Theme part height exceeds u16: {height}"))
        }),
    }
}

fn dmp2_region(chunk: &[u8], dmp2_pos: usize) -> &[u8] {
    let search_start = dmp2_pos.saturating_sub(0x40);
    if let Some(prev_kcbc_rel) = deepmap2::tile::find_bytes(&chunk[search_start..dmp2_pos], b"KCBC")
    {
        let kcbc_pos = search_start + prev_kcbc_rel;
        if kcbc_pos + 20 <= chunk.len() {
            let block_length =
                u32::from_le_bytes(chunk[kcbc_pos + 0x10..kcbc_pos + 0x14].try_into().unwrap())
                    as usize;
            let block_end = kcbc_pos.saturating_add(block_length);
            if block_length >= 0x20 && block_end > dmp2_pos && block_end <= chunk.len() {
                return &chunk[dmp2_pos..block_end];
            }
        }
    }

    if let Some(next_rel) = deepmap2::tile::find_bytes(&chunk[dmp2_pos + 4..], b"KCBC") {
        let next_kcbc = dmp2_pos + 4 + next_rel;
        return &chunk[dmp2_pos..next_kcbc];
    }

    &chunk[dmp2_pos..]
}

fn decode_theme_cbck(
    item: &crate::car::CSIItem,
    cbck: &RenditionThemeCBCK,
    options: &DecodeOptions,
) -> CarResult<Option<RenditionData>> {
    match &cbck.compression_type {
        CompressionType::Uncompressed => {
            let total: usize = cbck.chunks.iter().map(|c| c.raw_data.len()).sum();
            options.check_output_bytes(total, DecodeStage::Uncompressed)?;
            let mut data = Vec::with_capacity(total);
            for chunk in &cbck.chunks {
                data.extend_from_slice(chunk.raw_data.as_slice());
            }
            Ok(Some(RenditionData::Image { data }))
        }
        CompressionType::Lzfse => {
            let decode_chunk = |raw: &[u8]| -> CarResult<Vec<u8>> {
                let cap = raw.len().saturating_mul(16).max(4096);
                let mut output = Vec::with_capacity(cap);
                let mut decoder = LzfseRingDecoder::default();
                let n = decoder.decode_bytes(raw, &mut output).map_err(|_| {
                    CarError::DecodeFailed("LZFSE decompression failed".to_string())
                })?;
                if n == 0 && !raw.is_empty() {
                    return Err(CarError::DecodeFailed(
                        "LZFSE produced zero bytes for non-empty input".to_string(),
                    ));
                }
                output.truncate(n as usize);
                Ok(output)
            };

            // 顺序解压：避免嵌套并行与外层 extract par_iter 争抢线程池。
            let mut all_data = Vec::new();
            let mut total = 0usize;
            for chunk in &cbck.chunks {
                let part = decode_chunk(chunk.raw_data.as_slice())?;
                total = total.checked_add(part.len()).ok_or_else(|| {
                    CarError::DecodeFailed("LZFSE output length overflow".to_string())
                })?;
                options.check_output_bytes(total, DecodeStage::Lzfse)?;
                all_data.reserve(part.len());
                all_data.extend_from_slice(&part);
            }

            Ok(Some(RenditionData::Image { data: all_data }))
        }
        CompressionType::Zip => {
            let mut all_data = Vec::new();
            let mut total = 0usize;
            for chunk in &cbck.chunks {
                let mut decoder = MultiGzDecoder::new(chunk.raw_data.as_slice());
                let mut part = Vec::new();
                decoder.read_to_end(&mut part).map_err(|err| {
                    CarError::DecodeFailed(format!("Zip/gzip decompression failed: {err}"))
                })?;
                total = total.checked_add(part.len()).ok_or_else(|| {
                    CarError::DecodeFailed("Zip/gzip output length overflow".to_string())
                })?;
                options.check_output_bytes(total, DecodeStage::Zip)?;
                all_data.reserve(part.len());
                all_data.extend_from_slice(&part);
            }

            Ok(Some(RenditionData::Image { data: all_data }))
        }
        CompressionType::Lzvn => {
            let width = item.header.width as usize;
            let height = item.header.height as usize;
            // 与 Uncompressed/Lzfse 分支的契约一致：返回原始编码字节，
            // 按实际 encoding 的 bpp（以及可能存在的 BytesPerRow stride）计算
            // decoded_len —— lzvn::decode_raw 要求精确长度。
            let bpp = encoding_bytes_per_pixel(item.header.encoding).ok_or_else(|| {
                CarError::DecodeFailed(format!(
                    "LZVN decompression unsupported for encoding {:?}",
                    item.header.encoding
                ))
            })?;
            let min_row = width * bpp;
            // 优先使用 BytesPerRow TLV（可能大于 width*bpp，表示带行填充），
            // 否则回退到紧凑布局。to_image() 端会基于同一 TLV 剥离 stride。
            let bytes_per_row = match item.bytes_per_row_tlv() {
                Some(bpr) => {
                    let bpr = bpr as usize;
                    if bpr < min_row {
                        return Err(CarError::DecodeFailed(format!(
                            "bytes_per_row ({}) smaller than minimum row size ({})",
                            bpr, min_row
                        )));
                    }
                    bpr
                }
                None => min_row,
            };
            let expected_total = bytes_per_row * height;
            options.check_output_bytes(expected_total, DecodeStage::Lzvn)?;
            let mut all_data = Vec::with_capacity(expected_total);
            let is_single = cbck.chunks.len() == 1;
            for chunk in &cbck.chunks {
                // 单 chunk 对应整图，多 chunk 必须依据 part_header.height 计算本段行数。
                let expected_chunk = if is_single {
                    expected_total
                } else {
                    let chunk_height = chunk
                        .part_header
                        .as_ref()
                        .map(|h| h.height as usize)
                        .filter(|h| *h > 0)
                        .ok_or_else(|| {
                            CarError::DecodeFailed(
                                "LZVN multi-chunk missing part_header height".to_string(),
                            )
                        })?;
                    chunk_height * bytes_per_row
                };
                let decompressed =
                    deepmap2::codec::decompress_lzvn(chunk.raw_data.as_slice(), expected_chunk)?;
                all_data.extend_from_slice(&decompressed);
            }
            if all_data.len() != expected_total {
                return Err(CarError::DecodeFailed(format!(
                    "LZVN output size mismatch: expected {}, got {}",
                    expected_total,
                    all_data.len()
                )));
            }
            Ok(Some(RenditionData::Image { data: all_data }))
        }
        CompressionType::Deepmap2 => {
            let width = u16::try_from(item.header.width)
                .map_err(|_| CarError::DecodeFailed("CSI width exceeds u16".to_string()))?;
            let height = u16::try_from(item.header.height)
                .map_err(|_| CarError::DecodeFailed("CSI height exceeds u16".to_string()))?;
            let pixel_format_override = deepmap2_pixel_format_override(item.header.encoding);
            let mut all_data = Vec::new();
            options.check_output_bytes(
                (width as usize)
                    .saturating_mul(height as usize)
                    .saturating_mul(4),
                DecodeStage::Deepmap2,
            )?;
            let mut expected_width: Option<u16> = None;
            let mut decoded_height = 0u32;
            for chunk in &cbck.chunks {
                let output_height = chunk_output_height(chunk, cbck.chunks.len(), height)?;
                let chunk = chunk.raw_data.as_slice();
                let kcbc_pos = deepmap2::tile::find_bytes(chunk, b"KCBC");
                let dmp2_pos = deepmap2::tile::find_bytes(chunk, b"dmp2");
                let img = match (kcbc_pos, dmp2_pos) {
                    (Some(k), Some(d)) if k <= d => deepmap2::decode_kcbc_with_options(
                        &chunk[k..],
                        Some(width),
                        None,
                        pixel_format_override,
                    )?,
                    (Some(_), Some(d)) => deepmap2::decode_with_options(
                        dmp2_region(chunk, d),
                        Some(width),
                        output_height,
                        pixel_format_override,
                    )?,
                    (Some(k), None) => deepmap2::decode_kcbc_with_options(
                        &chunk[k..],
                        Some(width),
                        None,
                        pixel_format_override,
                    )?,
                    (None, Some(d)) => deepmap2::decode_with_options(
                        dmp2_region(chunk, d),
                        Some(width),
                        output_height,
                        pixel_format_override,
                    )?,
                    (None, None) => {
                        return Err(CarError::DecodeFailed(
                            "No deepmap2 magic found in chunk".to_string(),
                        ));
                    }
                };
                match expected_width {
                    None => expected_width = Some(img.width),
                    Some(w) if w != img.width => {
                        return Err(CarError::DecodeFailed(format!(
                            "Deepmap2 chunk width mismatch: expected {}, got {}",
                            w, img.width
                        )));
                    }
                    _ => {}
                }
                decoded_height = decoded_height.saturating_add(u32::from(img.height));
                all_data.extend_from_slice(&img.rgba);
                options.check_output_bytes(all_data.len(), DecodeStage::Deepmap2)?;
            }
            if cbck.chunks.len() > 1 && decoded_height != u32::from(height) {
                return Err(CarError::DecodeFailed(format!(
                    "Deepmap2 chunk height mismatch: expected {}, got {}",
                    height, decoded_height
                )));
            }
            Ok(Some(RenditionData::Image { data: all_data }))
        }
        _ => Err(CarError::UnsupportedCompression(cbck.compression_type)),
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::io::{Cursor, Write};

    use deku::{DekuReader, reader::Reader};
    use flate2::{Compression, write::GzEncoder};
    use std::path::PathBuf;

    use super::{
        DecodeBudgetError, DecodeOptions, DecodeStage, RenditionData, RenditionDataRef,
        deepmap2_pixel_format_override, dmp2_region,
    };
    use crate::car::{CSIItem, Car, CarError};
    use crate::model::CSIHeader;
    use crate::model::rendition::{AttributeType, CompressionType, LayoutType};

    const COMP_UNCOMPRESSED: u32 = 0;
    const COMP_ZIP: u32 = 2;
    const COMP_LZVN: u32 = 3;
    const COMP_LZFSE: u32 = 4;
    const COMP_HEVC: u32 = 9;
    const COMP_DEEPMAP2: u32 = 11;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../car-tests/data")
            .join(name)
    }

    fn fixture_car() -> Car {
        Car::new(fixture_path("Assets.car")).expect("load test Assets.car")
    }

    #[test]
    fn decode_data_returns_none_when_rendition_is_missing() {
        let item = build_item(2, 2, None);

        assert!(matches!(item.decode_data(), Ok(None)));
    }

    #[test]
    fn decode_data_rejects_unknown_rendition_variant() {
        let item = build_item(2, 2, Some(&build_unknown_rendition(*b"FAIL", b"raw")));

        let err = match item.decode_data() {
            Ok(_) => panic!("unknown rendition should fail"),
            Err(err) => err,
        };
        assert!(matches!(err, CarError::DecodeFailed(_)));
        assert!(format!("{err}").contains("Unknown rendition type"));
    }

    #[test]
    fn decode_data_rejects_invalid_lzfse_payload() {
        let item = build_item(
            2,
            2,
            Some(&build_theme_cbck_single(COMP_LZFSE, b"not-valid-lzfse")),
        );

        let err = match item.decode_data() {
            Ok(_) => panic!("invalid LZFSE should fail"),
            Err(err) => err,
        };
        assert!(matches!(err, CarError::DecodeFailed(_)));
        assert!(format!("{err}").contains("LZFSE"));
    }

    #[test]
    fn decode_data_rejects_unsupported_compression() {
        let item = build_item(
            2,
            2,
            Some(&build_theme_cbck_single(COMP_HEVC, b"hevc-bytes")),
        );

        let err = match item.decode_data() {
            Ok(_) => panic!("unsupported compression should fail"),
            Err(err) => err,
        };
        assert!(matches!(
            err,
            CarError::UnsupportedCompression(CompressionType::HEVC)
        ));
    }

    #[test]
    fn decode_data_decodes_zip_gzip_chunks() {
        let first = gzip_bytes(&[1, 2, 3, 4]);
        let second = gzip_bytes(&[5, 6, 7, 8]);
        let item = build_item(
            2,
            1,
            Some(&build_theme_cbck_multi(COMP_ZIP, &[&first, &second])),
        );

        let decoded = item.decode_data().expect("gzip Zip decode should succeed");
        let Some(RenditionData::Image { data }) = decoded else {
            panic!("expected RenditionData::Image");
        };
        assert_eq!(data, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn decode_data_rejects_invalid_zip_gzip_payload() {
        let item = build_item(
            2,
            1,
            Some(&build_theme_cbck_single(COMP_ZIP, b"not-valid-gzip")),
        );

        let err = item
            .decode_data()
            .expect_err("invalid gzip Zip payload should fail");
        assert!(
            matches!(err, CarError::DecodeFailed(_)),
            "expected DecodeFailed, got {err:?}"
        );
        assert!(format!("{err}").contains("Zip/gzip decompression failed"));
    }

    #[test]
    fn decode_data_with_options_rejects_uncompressed_output_over_budget() {
        let item = build_item(
            2,
            2,
            Some(&build_theme_cbck_single(COMP_UNCOMPRESSED, &[1, 2, 3, 4])),
        );
        let options = DecodeOptions::default().with_max_output_bytes(3);

        let err = item
            .decode_data_with_options(&options)
            .expect_err("decoded bytes should exceed budget");
        assert!(matches!(err, CarError::DecodeBudgetExceeded(_)));
    }

    #[test]
    fn decode_data_ref_with_options_rejects_raw_data_payload_over_budget() {
        let item = build_item(1, 1, Some(&build_raw_data_rendition(&[1, 2, 3, 4])));
        let options = DecodeOptions::default().with_max_output_bytes(3);

        let err = item
            .decode_data_ref_with_options(&options)
            .expect_err("borrowed raw data should exceed budget");
        assert_borrowed_payload_budget(err);
    }

    #[test]
    fn raw_data_payload_bytes_view_and_borrowed_decode() {
        let payload = [1, 2, 3, 4];
        let item = build_item(1, 1, Some(&build_raw_data_rendition(&payload)));
        let Some(crate::model::rendition::Rendition::RawData(raw)) = &item.header.rendition else {
            panic!("expected raw data rendition");
        };

        assert_eq!(raw.raw_data.as_slice(), payload);
        assert_eq!(raw.raw_data.to_vec(), payload);

        let decoded = item
            .decode_data_ref()
            .expect("raw data should decode")
            .expect("raw data should be present");
        let RenditionDataRef::RawData {
            data: Cow::Borrowed(bytes),
        } = decoded
        else {
            panic!("expected borrowed raw data");
        };
        assert_eq!(bytes, payload);
    }

    #[test]
    fn decode_data_with_options_rejects_color_payload_over_budget() {
        let item = build_item(1, 1, Some(&build_color_rendition(&[1.0])));
        let options = DecodeOptions::default().with_max_output_bytes(7);

        let err = item
            .decode_data_with_options(&options)
            .expect_err("borrowed color data should exceed budget");
        assert_borrowed_payload_budget(err);
    }

    #[test]
    fn decode_data_with_options_rejects_multisize_payload_over_budget() {
        let item = build_item(1, 1, Some(&build_multisize_rendition(&[(16, 16, 0, 0)])));
        let options = DecodeOptions::default().with_max_output_bytes(0);

        let err = item
            .decode_data_with_options(&options)
            .expect_err("borrowed multisize data should exceed budget");
        assert_borrowed_payload_budget(err);
    }

    #[test]
    fn decode_data_scans_for_offset_dmp2_magic() {
        let dmp2 = build_dmp2_none_g8(2, 1, &[1, 2]);
        let prefixed = build_prefixed_chunk(b"prefix", &dmp2);
        let item = build_item(
            2,
            1,
            Some(&build_theme_cbck_single(COMP_DEEPMAP2, &prefixed)),
        );

        let decoded = item.decode_data().expect("decode should succeed");
        let Some(RenditionData::Image { data }) = decoded else {
            panic!("expected image data");
        };
        assert_eq!(data, rgba_from_grays(&[1, 2]));
    }

    #[test]
    fn decode_data_scans_for_offset_kcbc_magic() {
        let dmp2 = build_dmp2_none_g8(2, 1, &[7, 8]);
        let kcbc = build_kcbc(&[(0, 0, 1, dmp2.as_slice())]);
        let prefixed = build_prefixed_chunk(b"wrapper", &kcbc);
        let item = build_item(
            2,
            1,
            Some(&build_theme_cbck_single(COMP_DEEPMAP2, &prefixed)),
        );

        let decoded = item.decode_data().expect("decode should succeed");
        let Some(RenditionData::Image { data }) = decoded else {
            panic!("expected image data");
        };
        assert_eq!(data, rgba_from_grays(&[7, 8]));
    }

    #[test]
    fn decode_data_prefers_earlier_dmp2_over_later_kcbc() {
        // dmp2 magic appears before a spurious KCBC sequence in the same chunk
        let dmp2 = build_dmp2_none_g8(2, 1, &[5, 6]);
        let mut chunk = Vec::new();
        chunk.extend_from_slice(b"junk");
        chunk.extend_from_slice(&dmp2);
        // Append KCBC bytes after the dmp2 payload — should be ignored
        chunk.extend_from_slice(b"KCBC");
        chunk.extend_from_slice(&[0u8; 16]);
        let item = build_item(2, 1, Some(&build_theme_cbck_single(COMP_DEEPMAP2, &chunk)));

        let decoded = item.decode_data().expect("decode should succeed");
        let Some(RenditionData::Image { data }) = decoded else {
            panic!("expected image data");
        };
        assert_eq!(data, rgba_from_grays(&[5, 6]));
    }

    #[test]
    fn decode_data_prefers_earlier_kcbc_over_later_dmp2() {
        // KCBC magic appears before a trailing dmp2 sequence
        let inner_dmp2 = build_dmp2_none_g8(2, 1, &[9, 10]);
        let kcbc = build_kcbc(&[(0, 0, 1, inner_dmp2.as_slice())]);
        let mut chunk = Vec::new();
        chunk.extend_from_slice(b"hdr");
        chunk.extend_from_slice(&kcbc);
        // Append a standalone dmp2 after KCBC — should be ignored
        let trailing_dmp2 = build_dmp2_none_g8(2, 1, &[99, 99]);
        chunk.extend_from_slice(&trailing_dmp2);
        let item = build_item(2, 1, Some(&build_theme_cbck_single(COMP_DEEPMAP2, &chunk)));

        let decoded = item.decode_data().expect("decode should succeed");
        let Some(RenditionData::Image { data }) = decoded else {
            panic!("expected image data");
        };
        assert_eq!(data, rgba_from_grays(&[9, 10]));
    }

    #[test]
    fn decode_data_reassembles_multi_chunk_deepmap2_strips() {
        let first = build_dmp2_none_g8(2, 1, &[1, 2]);
        let second = build_dmp2_none_g8(2, 1, &[3, 4]);
        let item = build_item(
            2,
            2,
            Some(&build_theme_cbck_multi(
                COMP_DEEPMAP2,
                &[first.as_slice(), second.as_slice()],
            )),
        );

        let decoded = item.decode_data().expect("decode should succeed");
        let Some(RenditionData::Image { data }) = decoded else {
            panic!("expected image data");
        };
        assert_eq!(data, rgba_from_grays(&[1, 2, 3, 4]));
    }

    #[test]
    fn decode_data_rejects_deepmap2_chunk_without_magic() {
        let item = build_item(
            2,
            1,
            Some(&build_theme_cbck_single(COMP_DEEPMAP2, b"missing-magic")),
        );

        let err = match item.decode_data() {
            Ok(_) => panic!("missing magic should fail"),
            Err(err) => err,
        };
        assert!(matches!(err, CarError::DecodeFailed(_)));
        assert!(format!("{err}").contains("magic"));
    }

    #[test]
    fn decode_data_rejects_invalid_multi_chunk_geometry() {
        let first = build_dmp2_none_g8(2, 1, &[1, 2]);
        let second = build_dmp2_none_g8(1, 1, &[3]);
        let item = build_item(
            2,
            2,
            Some(&build_theme_cbck_multi(
                COMP_DEEPMAP2,
                &[first.as_slice(), second.as_slice()],
            )),
        );

        let err = match item.decode_data() {
            Ok(_) => panic!("mismatched widths should fail"),
            Err(err) => err,
        };
        assert!(matches!(
            err,
            CarError::Deepmap2(_) | CarError::DecodeFailed(_)
        ));
    }

    #[test]
    fn decode_data_lzvn_honors_bytes_per_row_stride() {
        // 回归：LZVN 分支此前按 width*height*bpp 硬算 expected_total，忽略了
        // BytesPerRow TLV 声明的行步幅，导致带 stride 填充的合法资源
        // 在 lzvn::decode_raw 阶段就因长度不匹配而失败，根本走不到
        // to_image() 的去填充逻辑。
        //
        // 构造 2×3 GRAY（bpp=1）图像，BytesPerRow=4（每行 2 字节填充）。
        // 原始字节按 stride*height=12 组织，经 LZVN 压缩后作为 ThemeCBCK 载荷。
        let raw_pixels: [u8; 12] = [
            0x11, 0x22, 0x00, 0x00, // row 0: [0x11, 0x22] + 2 字节填充
            0x33, 0x44, 0x00, 0x00, // row 1
            0x55, 0x66, 0x00, 0x00, // row 2
        ];
        let compressed = lzvn::encode_raw(&raw_pixels);
        let rendition = build_theme_cbck_single(COMP_LZVN, &compressed);
        let tlv = bytes_per_row_tlv(4);
        let item = build_item_ex(2, 3, b"YARG", &tlv, Some(&rendition));

        let decoded = item
            .decode_data()
            .expect("LZVN decode with stride should succeed");
        let Some(RenditionData::Image { data }) = decoded else {
            panic!("expected RenditionData::Image");
        };
        // decode_data 应返回带 stride 的原始字节（与 Uncompressed/Lzfse 契约一致），
        // 由 to_image() 基于同一 BytesPerRow TLV 剥离填充。
        assert_eq!(data, raw_pixels);
    }

    #[test]
    fn decode_data_lzvn_rejects_bytes_per_row_below_minimum() {
        // BytesPerRow TLV 若小于 width*bpp 应直接报错，而不是尝试解码后静默出错。
        let raw_pixels: [u8; 4] = [0x11, 0x22, 0x33, 0x44];
        let compressed = lzvn::encode_raw(&raw_pixels);
        let rendition = build_theme_cbck_single(COMP_LZVN, &compressed);
        let tlv = bytes_per_row_tlv(1); // width=2 → min_row=2，但 TLV 声明 1
        let item = build_item_ex(2, 2, b"YARG", &tlv, Some(&rendition));

        let err = match item.decode_data() {
            Ok(_) => panic!("expected bytes_per_row underflow to fail"),
            Err(err) => err,
        };
        assert!(matches!(err, CarError::DecodeFailed(_)));
        assert!(format!("{err}").contains("bytes_per_row"));
    }

    #[test]
    fn decode_data_decodes_real_assets_car_deepmap2_problem_cases() {
        let car = fixture_car();
        for name in [
            "2016_coin1",
            "AirKissHelper",
            "CarPlayCallRecord",
            "CardPkgGiftCardBackGroundImage",
            "ChatMessagesPreviewSelectZoomBorder",
            "DeleteData_Loading",
        ] {
            let item = car
                .rendtions_with_name(name)
                .and_then(|items| {
                    items
                        .iter()
                        .find(|item| !matches!(item.layout(), LayoutType::InternalReference))
                })
                .unwrap_or_else(|| panic!("missing fixture rendition: {name}"));
            let decoded = item
                .decode_data()
                .unwrap_or_else(|err| panic!("decode should succeed for {name}: {err}"));
            assert!(
                matches!(decoded, Some(RenditionData::Image { .. })),
                "{name} should decode to image data"
            );
        }

        let item = car
            .rendtions_with_name("live_playmembers_background")
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| !matches!(item.layout(), LayoutType::InternalReference))
            })
            .expect("missing fixture rendition: live_playmembers_background");
        let decoded = item.decode_data().unwrap_or_else(|err| {
            panic!("decode should succeed for live_playmembers_background: {err}")
        });
        let Some(RenditionData::Image { data }) = decoded else {
            panic!("live_playmembers_background should decode to image data");
        };
        assert_eq!(data.len(), 1170 * 828 * 4);
    }

    #[test]
    fn decode_data_decodes_bootimage_ipad_palette_lzvn_tail_chunk() {
        let car = fixture_car();
        let item = car
            .rendtions_with_name("BootImage")
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.name() == "Default@2x~ipad.png")
            })
            .expect("missing BootImage Default@2x~ipad.png rendition");

        let decoded = item
            .decode_data()
            .unwrap_or_else(|err| panic!("decode should succeed for BootImage iPad: {err}"));
        let Some(RenditionData::Image { data }) = decoded else {
            panic!("BootImage iPad should decode to image data");
        };
        assert_eq!(data.len(), 1536 * 2048 * 4);
    }

    #[test]
    fn decode_data_decodes_real_assets_car_zip_app_icons() {
        let car = fixture_car();
        let items = car
            .rendtions_with_name("AppIcon")
            .expect("AppIcon fixture facet");

        for (name, expected_len) in [
            ("AppIcon_1024.png", 1024 * 1024 * 4),
            ("AppIcon_1024_dark.png", 1024 * 1024 * 4),
            ("AppIcon_1024_tinted.png", 1024 * 1024 * 2),
        ] {
            let variants = items
                .iter()
                .filter(|item| item.name() == name)
                .collect::<Vec<_>>();
            assert_eq!(
                variants.len(),
                2,
                "{name} should have phone and pad variants"
            );

            let mut decoded_by_idiom = Vec::new();
            for item in variants {
                assert_eq!(item.compression(), Some(CompressionType::Zip));
                assert_eq!(item.unsupported_reason(), None);
                let idiom = item
                    .attributes()
                    .iter()
                    .find(|attr| attr.name == AttributeType::Idiom)
                    .map(|attr| attr.val)
                    .expect("AppIcon Zip variant should carry idiom");
                let decoded = item
                    .decode_data()
                    .unwrap_or_else(|err| panic!("Zip decode should succeed for {name}: {err}"));
                let Some(RenditionData::Image { data }) = decoded else {
                    panic!("{name} should decode to image data");
                };
                assert_eq!(data.len(), expected_len, "{name} decoded length");
                decoded_by_idiom.push((idiom, item.key_values().to_vec(), data));
            }

            decoded_by_idiom.sort_by_key(|(idiom, _, _)| *idiom);
            assert_eq!(
                decoded_by_idiom[0].0, 1,
                "{name} first idiom should be phone"
            );
            assert_eq!(
                decoded_by_idiom[1].0, 2,
                "{name} second idiom should be pad"
            );
            assert_ne!(
                decoded_by_idiom[0].1, decoded_by_idiom[1].1,
                "{name} phone and pad keys should differ"
            );
            assert_eq!(
                decoded_by_idiom[0].2, decoded_by_idiom[1].2,
                "{name} phone and pad payloads should match"
            );
        }
    }

    #[test]
    fn deepmap2_pixel_format_override_matches_csi_mapping() {
        assert_eq!(
            deepmap2_pixel_format_override(crate::model::Encoding::ARGB),
            Some(deepmap2::PixelFormat::Rgba8888)
        );
        assert_eq!(
            deepmap2_pixel_format_override(crate::model::Encoding::ARGB16),
            Some(deepmap2::PixelFormat::Rgba8888)
        );
        assert_eq!(
            deepmap2_pixel_format_override(crate::model::Encoding::GA8),
            Some(deepmap2::PixelFormat::GA88)
        );
    }

    #[test]
    fn dmp2_region_stops_before_next_kcbc() {
        let mut chunk = Vec::new();
        chunk.extend_from_slice(b"prefix");
        let dmp2_pos = chunk.len();
        chunk.extend_from_slice(b"dmp2");
        chunk.extend_from_slice(&[1, 2, 3, 4]);
        chunk.extend_from_slice(b"KCBC");
        chunk.extend_from_slice(&[9, 9, 9, 9]);

        assert_eq!(dmp2_region(&chunk, dmp2_pos), b"dmp2\x01\x02\x03\x04");
    }

    fn build_item(width: u32, height: u32, rendition: Option<&[u8]>) -> CSIItem {
        build_item_ex(width, height, b"ATAD", &[], rendition)
    }

    fn build_item_ex(
        width: u32,
        height: u32,
        encoding: &[u8; 4],
        tlv: &[u8],
        rendition: Option<&[u8]>,
    ) -> CSIItem {
        let bytes = build_csi_header(width, height, encoding, tlv, rendition);
        let mut reader = Reader::new(Cursor::new(bytes));
        let header =
            CSIHeader::from_reader_with_ctx(&mut reader, ()).expect("test header should decode");
        CSIItem {
            attrs: Vec::new(),
            header,
            key_values: Box::default(),
        }
    }

    fn build_csi_header(
        width: u32,
        height: u32,
        encoding: &[u8; 4],
        tlv: &[u8],
        rendition: Option<&[u8]>,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"ISTC");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&100u32.to_le_bytes());
        bytes.extend_from_slice(encoding);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&1000u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 128]);
        bytes.extend_from_slice(&(tlv.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(rendition.map_or(0, |data| data.len()) as u32).to_le_bytes());
        bytes.extend_from_slice(tlv);
        if let Some(rendition) = rendition {
            bytes.extend_from_slice(rendition);
        }
        bytes
    }

    fn bytes_per_row_tlv(bpr: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1007u32.to_le_bytes()); // BytesPerRow tag
        bytes.extend_from_slice(&4u32.to_le_bytes()); // length
        bytes.extend_from_slice(&bpr.to_le_bytes());
        bytes
    }

    fn assert_borrowed_payload_budget(err: CarError) {
        match err {
            CarError::DecodeBudgetExceeded(DecodeBudgetError::OutputBytesExceeded {
                stage: DecodeStage::BorrowedPayload,
                ..
            }) => {}
            err => panic!("expected borrowed payload budget error, got {err:?}"),
        }
    }

    fn build_color_rendition(components: &[f64]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RLOC");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes()); // ColorSpace::SRGB
        bytes.extend_from_slice(&(components.len() as u32).to_le_bytes());
        for component in components {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
        bytes
    }

    fn build_raw_data_rendition(raw_data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"DWAR");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(raw_data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(raw_data);
        bytes
    }

    fn build_multisize_rendition(entries: &[(u32, u32, u16, u16)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"SISM");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for &(width, height, index, idiom) in entries {
            bytes.extend_from_slice(&width.to_le_bytes());
            bytes.extend_from_slice(&height.to_le_bytes());
            bytes.extend_from_slice(&index.to_le_bytes());
            bytes.extend_from_slice(&idiom.to_le_bytes());
        }
        bytes
    }

    fn build_unknown_rendition(tag: [u8; 4], raw_data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&tag);
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(raw_data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(raw_data);
        bytes
    }

    fn build_theme_cbck_single(compression: u32, chunk: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MLEC");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&compression.to_le_bytes());
        bytes.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        bytes.extend_from_slice(chunk);
        bytes
    }

    fn build_theme_cbck_multi(compression: u32, chunks: &[&[u8]]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MLEC");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&compression.to_le_bytes());
        bytes.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
        for chunk in chunks {
            bytes.extend_from_slice(b"PART");
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
            bytes.extend_from_slice(chunk);
        }
        bytes
    }

    fn gzip_bytes(raw: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(raw).expect("write gzip test payload");
        encoder.finish().expect("finish gzip test payload")
    }

    fn build_prefixed_chunk(prefix: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(prefix);
        bytes.extend_from_slice(payload);
        bytes
    }

    fn build_dmp2_none_g8(width: u16, height: u16, pixels: &[u8]) -> Vec<u8> {
        assert_eq!(pixels.len(), width as usize * height as usize);
        let mut data = Vec::new();
        data.extend_from_slice(b"dmp2");
        data.push(0x01);
        data.push(0x00);
        data.push(0x00);
        data.push(0x01);
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(&height.to_le_bytes());
        data.extend_from_slice(pixels);
        data
    }

    fn build_kcbc(tiles: &[(u32, u32, u32, &[u8])]) -> Vec<u8> {
        let mut data = Vec::new();
        for &(row, col, row_hint, dmp2) in tiles {
            let block_len = (20 + dmp2.len()) as u32;
            data.extend_from_slice(b"KCBC");
            data.extend_from_slice(&col.to_le_bytes());
            data.extend_from_slice(&row.to_le_bytes());
            data.extend_from_slice(&row_hint.to_le_bytes());
            data.extend_from_slice(&block_len.to_le_bytes());
            data.extend_from_slice(dmp2);
        }
        data
    }

    fn rgba_from_grays(values: &[u8]) -> Vec<u8> {
        let mut data = Vec::with_capacity(values.len() * 4);
        for value in values {
            data.extend_from_slice(&[*value, *value, *value, 0xFF]);
        }
        data
    }

    fn lzfse_compress(data: &[u8]) -> Vec<u8> {
        let mut encoder = lzfse_rust::LzfseRingEncoder::default();
        let mut out = Vec::new();
        encoder.encode_bytes(data, &mut out).expect("lzfse encode");
        out
    }

    #[test]
    fn decode_data_lzfse_multi_chunk_preserves_order() {
        // 两个独立 LZFSE chunk，解压后应按原顺序拼接
        let chunk0 = lzfse_compress(&[1u8, 2, 3]);
        let chunk1 = lzfse_compress(&[4u8, 5, 6]);
        let item = build_item(
            3,
            2,
            Some(&build_theme_cbck_multi(
                COMP_LZFSE,
                &[chunk0.as_slice(), chunk1.as_slice()],
            )),
        );
        let decoded = item
            .decode_data()
            .expect("multi-chunk LZFSE should succeed");
        let Some(RenditionData::Image { data }) = decoded else {
            panic!("expected Image data");
        };
        assert_eq!(data, [1u8, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn decode_data_lzfse_multi_chunk_fails_on_bad_chunk() {
        // 第一个 chunk 合法，第二个 chunk 损坏，整体应报 DecodeFailed
        let good = lzfse_compress(&[1u8, 2, 3]);
        let bad = b"not-valid-lzfse-data".as_slice();
        let item = build_item(
            3,
            2,
            Some(&build_theme_cbck_multi(COMP_LZFSE, &[good.as_slice(), bad])),
        );
        let err = item.decode_data().expect_err("bad LZFSE chunk should fail");
        assert!(matches!(err, CarError::DecodeFailed(_)));
        assert!(format!("{err}").contains("LZFSE"));
    }
}
