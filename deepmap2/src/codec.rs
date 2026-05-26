use lzfse_rust::LzfseRingDecoder;
use util::apple_compression::{self, AppleCompressionError};

use crate::color::{output_bytes_to_rgba, palette_entry_to_rgba, row_to_rgba};
use crate::model::{DecodeType, Deepmap2Header, PixelFormat};
use crate::predictor::apply_predictor;
use crate::{Deepmap2Error, Deepmap2Result};

/// Apple Deepmap2 Lossless 算法选择阈值:
/// 期望输出大小 < 0x1000 → LZVN，否则 → LZFSE
const LZVN_THRESHOLD: usize = 0x1000;
const APPLE_STREAM_SCAN_LIMIT: usize = 8;
const APPLE_STREAM_EOS_MAGIC: &[u8; 4] = b"bvx$";

fn apply_pixel_format_override(
    header: &Deepmap2Header,
    pixel_format_override: Option<PixelFormat>,
) -> Deepmap2Header {
    let mut header = header.clone();
    if let Some(pixel_format) = pixel_format_override {
        header.pixel_format = pixel_format;
    }
    header
}

fn apple_stream_slice(data: &[u8]) -> &[u8] {
    if apple_compression::is_apple_compression_stream(data) {
        return data;
    }

    let scan_limit = data.len().min(APPLE_STREAM_SCAN_LIMIT);
    if let Some(marker) = data[..scan_limit]
        .windows(4)
        .position(apple_compression::is_apple_compression_stream)
    {
        &data[marker..]
    } else {
        data
    }
}

fn map_apple_compression_error(error: AppleCompressionError) -> Deepmap2Error {
    match error {
        AppleCompressionError::NativeFallbackUnavailable(message) => {
            Deepmap2Error::NativeFallbackUnavailable(message)
        }
        AppleCompressionError::DecodeFailed(_) | AppleCompressionError::Io(_) => {
            Deepmap2Error::LzfseDecompress
        }
    }
}

fn is_apple_stream_start_magic(data: &[u8]) -> bool {
    matches!(data.get(..4), Some(b"bvx2") | Some(b"bvxn") | Some(b"bvx-"))
}

fn decode_lzfse_rust_only(data: &[u8]) -> Option<Vec<u8>> {
    let cap = data.len().saturating_mul(16).max(4096);
    let mut out = Vec::with_capacity(cap);
    let mut decoder = LzfseRingDecoder::default();
    if let Ok(n) = decoder.decode_bytes(data, &mut out)
        && (n > 0 || data.is_empty())
    {
        out.truncate(n as usize);
        Some(out)
    } else {
        None
    }
}

fn find_concatenated_apple_segment_boundary(data: &[u8]) -> Option<(usize, usize)> {
    let mut search_offset = 0usize;
    while search_offset + APPLE_STREAM_EOS_MAGIC.len() <= data.len() {
        let rel = data[search_offset..]
            .windows(APPLE_STREAM_EOS_MAGIC.len())
            .position(|window| window == APPLE_STREAM_EOS_MAGIC)?;
        let eos_pos = search_offset + rel;
        let next_scan_start = eos_pos + APPLE_STREAM_EOS_MAGIC.len();
        let next_scan_end = data.len().min(next_scan_start + APPLE_STREAM_SCAN_LIMIT);

        if let Some(next_rel) = data[next_scan_start..next_scan_end]
            .windows(4)
            .position(is_apple_stream_start_magic)
        {
            return Some((next_scan_start, next_scan_start + next_rel));
        }

        search_offset = next_scan_start;
    }

    None
}

fn decompress_concatenated_apple_stream(data: &[u8]) -> Deepmap2Result<Vec<u8>> {
    let mut decoded = Vec::new();
    let mut cursor = 0usize;

    loop {
        let remaining = &data[cursor..];
        let segment = apple_stream_slice(remaining);
        let start_offset = remaining.len().saturating_sub(segment.len());
        if start_offset > APPLE_STREAM_SCAN_LIMIT
            || !apple_compression::is_apple_compression_stream(segment)
        {
            return Err(Deepmap2Error::LzfseDecompress);
        }
        cursor += start_offset;

        if let Some((segment_end, next_segment_start)) =
            find_concatenated_apple_segment_boundary(&data[cursor..])
        {
            let segment = &data[cursor..cursor + segment_end];
            let part = decode_lzfse_rust_only(segment).ok_or(Deepmap2Error::LzfseDecompress)?;
            decoded.extend_from_slice(&part);
            cursor += next_segment_start;
            continue;
        }

        let part = decode_lzfse_rust_only(&data[cursor..]).ok_or(Deepmap2Error::LzfseDecompress)?;
        decoded.extend_from_slice(&part);
        return Ok(decoded);
    }
}

/// LZFSE 解压，返回解压后字节（错误时返回 LzfseDecompress）
fn decompress_lzfse(data: &[u8]) -> Deepmap2Result<Vec<u8>> {
    let data = apple_stream_slice(data);
    let is_apple_stream = apple_compression::is_apple_compression_stream(data);
    if let Some(decoded) = decode_lzfse_rust_only(data) {
        return Ok(decoded);
    }

    if !is_apple_stream {
        return Err(Deepmap2Error::LzfseDecompress);
    }

    if let Ok(decoded) = decompress_concatenated_apple_stream(data) {
        return Ok(decoded);
    }

    apple_compression::decode_lzfse_with_fallback(data).map_err(map_apple_compression_error)
}

/// LZVN 解压（raw stream），返回解压后字节
pub fn decompress_lzvn(data: &[u8], expected_output_size: usize) -> Deepmap2Result<Vec<u8>> {
    lzvn::decode_raw(data, expected_output_size).map_err(|_| Deepmap2Error::LzvnDecompress)
}

/// 尝试将 payload 解析为 size-prefixed 分块格式
///
/// 格式: [u32 LE chunk_size][chunk_data] 重复，直至消耗完所有字节。
/// 若不满足则返回 None（回退到整体处理）。
fn parse_size_prefixed_chunks(payload: &[u8]) -> Option<Vec<&[u8]>> {
    let mut chunks = Vec::new();
    let mut cursor = 0usize;
    while cursor + 4 <= payload.len() {
        let chunk_size = u32::from_le_bytes(payload[cursor..cursor + 4].try_into().ok()?) as usize;
        cursor += 4;
        if chunk_size == 0 || cursor + chunk_size > payload.len() {
            return None;
        }
        chunks.push(&payload[cursor..cursor + chunk_size]);
        cursor += chunk_size;
    }
    if cursor != payload.len() || chunks.is_empty() {
        return None;
    }
    Some(chunks)
}

fn decompress_lzfse_or_lzvn(data: &[u8], expected_output_size: usize) -> Deepmap2Result<Vec<u8>> {
    match decompress_lzfse(data) {
        Ok(bytes) => Ok(bytes),
        Err(err @ Deepmap2Error::LzfseDecompress) => {
            decompress_lzvn(data, expected_output_size).map_err(|_| err)
        }
        Err(err) => Err(err),
    }
}

/// 解码 Default 路径（Type 2）的解压后数据为 RGBA
///
/// 解压后数据布局:
/// - alpha_plane:    [width * height] bytes（仅 has_alpha 格式）
/// - predictor_bytes: [height] bytes（每行一个预测器类型）
/// - high_stream:    [width * height * components] bytes
/// - low_stream:     [width * height * components] bytes
fn decode_default_decompressed(
    header: &Deepmap2Header,
    decompressed: &[u8],
    width: usize,
    height: usize,
) -> Deepmap2Result<Vec<u8>> {
    let (rgba, _) =
        decode_default_decompressed_with_state(header, decompressed, width, height, None)?;
    Ok(rgba)
}

fn decode_default_decompressed_with_state(
    header: &Deepmap2Header,
    decompressed: &[u8],
    width: usize,
    height: usize,
    mut prev_row: Option<Vec<i16>>,
) -> Deepmap2Result<(Vec<u8>, Option<Vec<i16>>)> {
    let pixel_format = header.pixel_format;
    let has_alpha = pixel_format.has_alpha();
    let components = pixel_format.split_stream_components();
    let pixel_count = width * height;
    let alpha_size = if has_alpha { pixel_count } else { 0 };
    let split_count = pixel_count * components;

    let predictor_offset = alpha_size;
    let predictor_end = predictor_offset + height;
    let high_offset = predictor_end;
    let high_end = high_offset + split_count;
    let low_offset = high_end;
    let low_end = low_offset + split_count;

    if decompressed.len() < low_end {
        return Err(Deepmap2Error::InvalidData(format!(
            "Default 解压后数据不足: 需要 {} 字节，实际 {}",
            low_end,
            decompressed.len()
        )));
    }

    let alpha_plane = if has_alpha {
        Some(&decompressed[..alpha_size])
    } else {
        None
    };
    let predictor_bytes = &decompressed[predictor_offset..predictor_end];
    let high_stream = &decompressed[high_offset..high_end];
    let low_stream = &decompressed[low_offset..low_end];

    let split_row_width = width * components;
    let chroma_scale = header.chroma_scale();
    let mut rgba = vec![0u8; pixel_count * 4];

    for row in 0..height {
        let row_offset = row * split_row_width;
        let high_row = &high_stream[row_offset..row_offset + split_row_width];
        let low_row = &low_stream[row_offset..row_offset + split_row_width];

        let decoded_split_row: Vec<i16> = low_row
            .iter()
            .zip(high_row.iter())
            .map(|(&lo, &hi)| {
                let combined = (lo as u16) | ((hi as u16) << 8);
                let magnitude = (combined >> 1) as i16;
                // Apple deepmap2 zigzag: 偶数编码正值，奇数编码负值（负值 = -magnitude）
                if combined & 1 != 0 {
                    -magnitude
                } else {
                    magnitude
                }
            })
            .collect();

        // 灰度格式需要扩展为 3 分量组 [Y, 0, 0]，与 predictor 和 row_to_rgba 的
        // PREDICTOR_GROUP_SIZE=3 保持一致（匹配 Python 参考实现的 Deepmap2Coder 行为）
        let decoded_split_row: Vec<i16> = if !pixel_format.is_color() {
            decoded_split_row
                .iter()
                .flat_map(|&y| [y, 0i16, 0i16])
                .collect()
        } else {
            decoded_split_row
        };

        let pred_type = predictor_bytes[row];
        let predicted_row = apply_predictor(pred_type, &decoded_split_row, prev_row.as_deref());

        let alpha_row = alpha_plane.map(|ap| &ap[row * width..(row + 1) * width]);
        let rgba_row = row_to_rgba(pixel_format, &predicted_row, alpha_row, chroma_scale);

        let base = row * width * 4;
        rgba[base..base + rgba_row.len()].copy_from_slice(&rgba_row);

        prev_row = Some(predicted_row);
    }

    Ok((rgba, prev_row))
}

/// Type 2 路径: LZFSE 压缩多流 + zigzag + 预测器 + YCoCg → RGBA
fn decode_default(
    header: &Deepmap2Header,
    payload: &[u8],
    width: usize,
    height: usize,
) -> Deepmap2Result<Vec<u8>> {
    let decompressed = decompress_lzfse(payload)?;
    decode_default_decompressed(header, &decompressed, width, height)
}

fn decode_default_with_state(
    header: &Deepmap2Header,
    payload: &[u8],
    width: usize,
    height: usize,
    prev_row: Option<Vec<i16>>,
) -> Deepmap2Result<(Vec<u8>, Option<Vec<i16>>)> {
    let decompressed = decompress_lzfse(payload)?;
    decode_default_decompressed_with_state(header, &decompressed, width, height, prev_row)
}

/// Type 1/3 路径: 原始或压缩的像素数据 → RGBA
///
/// Type 3 (Lossless) 按 Apple _Deepmap2DecodeLossless 阈值选算法:
/// - 期望输出 < 0x1000 → 优先 LZVN，失败时回退 LZFSE
/// - 期望输出 >= 0x1000 → LZFSE
///
/// Lossless header 不携带具体压缩算法，因此对小尺寸 payload
/// 必须保留 LZFSE 回退以兼容实际以 LZFSE 压缩的小图。
fn decode_none_or_lossless(
    header: &Deepmap2Header,
    payload: &[u8],
    compressed: bool,
    width: usize,
    height: usize,
) -> Deepmap2Result<Vec<u8>> {
    let pixel_format = header.pixel_format;
    let expected = width * height * pixel_format.bytes_per_pixel();
    let output_bytes = if compressed {
        if expected < LZVN_THRESHOLD {
            match decompress_lzvn(payload, expected) {
                Ok(bytes) => bytes,
                Err(_) => decompress_lzfse(payload)?,
            }
        } else {
            decompress_lzfse(payload)?
        }
    } else {
        payload.to_vec()
    };
    if output_bytes.len() < expected {
        return Err(Deepmap2Error::InvalidData(format!(
            "输出数据不足: 期望 {} 字节，实际 {}",
            expected,
            output_bytes.len()
        )));
    }
    Ok(output_bytes_to_rgba(
        pixel_format,
        width,
        height,
        &output_bytes[..expected],
    ))
}

/// Type 4 路径: 调色板索引 → RGBA
fn decode_palette(
    header: &Deepmap2Header,
    payload: &[u8],
    width: usize,
    height: usize,
) -> Deepmap2Result<Vec<u8>> {
    let palette_size = header.palette_size.unwrap_or(0) as usize;
    let palette_type = header.palette_type.unwrap_or(0);
    if palette_size == 0 {
        return Err(Deepmap2Error::InvalidData(
            "Palette 模式缺少调色板".to_string(),
        ));
    }
    let palette = &header.palette;

    let pixel_count = width * height;
    let expected = match palette_type {
        3 => pixel_count * 2,
        4 => pixel_count,
        other => return Err(Deepmap2Error::InvalidPaletteType(other)),
    };
    let decompressed = if let Some(chunks) = parse_size_prefixed_chunks(payload) {
        let mut out = Vec::with_capacity(expected);
        for chunk in chunks {
            if out.len() >= expected {
                break;
            }
            let remaining = expected.saturating_sub(out.len());
            let part = decompress_lzfse_or_lzvn(chunk, remaining)?;
            out.extend_from_slice(&part);
        }
        out
    } else {
        decompress_lzfse_or_lzvn(payload, expected)?
    };

    match palette_type {
        3 => {
            // alpha_plane + index_plane，各 pixel_count 字节
            let needed = expected;
            if decompressed.len() < needed {
                return Err(Deepmap2Error::InvalidData(format!(
                    "Palette type3 数据不足: 需要 {} 字节，实际 {}",
                    needed,
                    decompressed.len()
                )));
            }
            let alpha_plane = &decompressed[..pixel_count];
            let index_plane = &decompressed[pixel_count..pixel_count * 2];
            let mut rgba = vec![0u8; pixel_count * 4];
            for (i, (&idx, &alpha)) in index_plane.iter().zip(alpha_plane.iter()).enumerate() {
                let entry = palette.get(idx as usize).copied().unwrap_or(0);
                let bytes = palette_entry_to_rgba(entry, Some(alpha));
                rgba[i * 4..i * 4 + 4].copy_from_slice(&bytes);
            }
            Ok(rgba)
        }
        4 => {
            // 纯索引，pixel_count 字节
            let needed = expected;
            if decompressed.len() < needed {
                return Err(Deepmap2Error::InvalidData(format!(
                    "Palette type4 数据不足: 需要 {} 字节，实际 {}",
                    needed,
                    decompressed.len()
                )));
            }
            let mut rgba = vec![0u8; pixel_count * 4];
            for (i, &idx) in decompressed[..needed].iter().enumerate() {
                let entry = palette.get(idx as usize).copied().unwrap_or(0);
                let bytes = palette_entry_to_rgba(entry, None);
                rgba[i * 4..i * 4 + 4].copy_from_slice(&bytes);
            }
            Ok(rgba)
        }
        other => Err(Deepmap2Error::InvalidPaletteType(other)),
    }
}

/// 处理 size-prefixed 分块 payload（Default 路径）
///
/// source_height 从解压数据大小反推，最后一个 chunk 可能含填充行导致
/// sum(source_heights) > total_height，因此已达目标高度后 break 跳过剩余 chunk。
fn decode_size_prefixed_default(
    header: &Deepmap2Header,
    chunks: &[&[u8]],
    width: usize,
    total_height: usize,
) -> Deepmap2Result<Vec<u8>> {
    let pixel_format = header.pixel_format;
    let components = pixel_format.split_stream_components();
    let has_alpha = pixel_format.has_alpha();

    // row_stride 用于从解压后数据大小反推行数
    let row_stride = (if has_alpha { width } else { 0 }) + 1 + width * components * 2;

    let mut rgba = vec![0u8; width * total_height * 4];
    let mut cursor_height = 0usize;

    for chunk in chunks {
        let remaining = total_height - cursor_height;
        if remaining == 0 {
            break;
        }
        let decompressed = decompress_lzfse(chunk)?;
        let source_height = decompressed.len().checked_div(row_stride).unwrap_or(0);
        if source_height == 0 {
            return Err(Deepmap2Error::InvalidData(
                "无法从 Default chunk 推断有效高度".to_string(),
            ));
        }
        let chunk_h = source_height.min(remaining);

        let alpha_size = if has_alpha { width * chunk_h } else { 0 };
        let predictor_size = chunk_h;
        let split_count = width * components * chunk_h;
        let needed = alpha_size + predictor_size + split_count * 2;
        let truncated: &[u8] = if decompressed.len() >= needed {
            &decompressed[..needed]
        } else {
            decompressed.as_slice()
        };

        let chunk_rgba = decode_default_decompressed(header, truncated, width, chunk_h)?;
        let copy_len = (width * chunk_h * 4).min(chunk_rgba.len());
        let dst = cursor_height * width * 4;
        rgba[dst..dst + copy_len].copy_from_slice(&chunk_rgba[..copy_len]);
        cursor_height += chunk_h;
    }

    if cursor_height != total_height {
        return Err(Deepmap2Error::InvalidData(format!(
            "size-prefixed 容器输出高度不足: 期望 {total_height}，实际 {cursor_height}"
        )));
    }

    Ok(rgba)
}

fn decode_size_prefixed_default_with_state(
    header: &Deepmap2Header,
    chunks: &[&[u8]],
    width: usize,
    total_height: usize,
    mut prev_row: Option<Vec<i16>>,
) -> Deepmap2Result<(Vec<u8>, Option<Vec<i16>>)> {
    let pixel_format = header.pixel_format;
    let components = pixel_format.split_stream_components();
    let has_alpha = pixel_format.has_alpha();

    // row_stride 用于从解压后数据大小反推行数
    let row_stride = (if has_alpha { width } else { 0 }) + 1 + width * components * 2;

    let mut rgba = vec![0u8; width * total_height * 4];
    let mut cursor_height = 0usize;

    for chunk in chunks {
        let remaining = total_height - cursor_height;
        if remaining == 0 {
            break;
        }
        let decompressed = decompress_lzfse(chunk)?;
        let source_height = decompressed.len().checked_div(row_stride).unwrap_or(0);
        if source_height == 0 {
            return Err(Deepmap2Error::InvalidData(
                "无法从 Default chunk 推断有效高度".to_string(),
            ));
        }
        let chunk_h = source_height.min(remaining);

        let alpha_size = if has_alpha { width * chunk_h } else { 0 };
        let predictor_size = chunk_h;
        let split_count = width * components * chunk_h;
        let needed = alpha_size + predictor_size + split_count * 2;
        let truncated: &[u8] = if decompressed.len() >= needed {
            &decompressed[..needed]
        } else {
            decompressed.as_slice()
        };

        let (chunk_rgba, next_prev_row) =
            decode_default_decompressed_with_state(header, truncated, width, chunk_h, prev_row)?;
        let copy_len = (width * chunk_h * 4).min(chunk_rgba.len());
        let dst = cursor_height * width * 4;
        rgba[dst..dst + copy_len].copy_from_slice(&chunk_rgba[..copy_len]);
        cursor_height += chunk_h;
        prev_row = next_prev_row;
    }

    if cursor_height != total_height {
        return Err(Deepmap2Error::InvalidData(format!(
            "size-prefixed 容器输出高度不足: 期望 {total_height}，实际 {cursor_height}"
        )));
    }

    Ok((rgba, prev_row))
}

/// 处理 size-prefixed 分块 payload（None / Lossless 路径）
#[allow(dead_code)]
fn decode_size_prefixed_none_or_lossless(
    header: &Deepmap2Header,
    chunks: &[&[u8]],
    compressed: bool,
    width: usize,
    total_height: usize,
) -> Deepmap2Result<Vec<u8>> {
    // 匹配参考实现: source_heights = [header.height] * len(chunks)
    let source_height = header.height as usize;

    let mut rgba = vec![0u8; width * total_height * 4];
    let mut cursor_height = 0usize;

    for chunk in chunks {
        let remaining = total_height - cursor_height;
        if remaining == 0 {
            return Err(Deepmap2Error::InvalidData(
                "size-prefixed 容器包含超出目标高度的额外 chunk".to_string(),
            ));
        }
        let chunk_height = source_height.min(remaining);
        let chunk_rgba = decode_none_or_lossless(header, chunk, compressed, width, chunk_height)?;
        let copy_len = (width * chunk_height * 4).min(chunk_rgba.len());
        let dst = cursor_height * width * 4;
        rgba[dst..dst + copy_len].copy_from_slice(&chunk_rgba[..copy_len]);
        cursor_height += chunk_height;
    }

    if cursor_height != total_height {
        return Err(Deepmap2Error::InvalidData(format!(
            "size-prefixed 容器输出高度不足: 期望 {total_height}，实际 {cursor_height}"
        )));
    }

    Ok(rgba)
}

/// 解码 payload 为 RGBA（考虑 size-prefixed 分块）
fn decode_payload(
    header: &Deepmap2Header,
    payload: &[u8],
    width: usize,
    height: usize,
) -> Deepmap2Result<Vec<u8>> {
    let decode_type = header.decode_type;

    // 拒绝不支持的像素格式，避免静默产出空数据
    if let PixelFormat::Unknown { tag } = header.pixel_format {
        return Err(Deepmap2Error::UnsupportedPixelFormat(tag));
    }

    match decode_type {
        DecodeType::Default => {
            if let Some(chunks) = parse_size_prefixed_chunks(payload) {
                decode_size_prefixed_default(header, &chunks, width, height)
            } else {
                decode_default(header, payload, width, height)
            }
        }
        DecodeType::Lossless => {
            if let Some(chunks) = parse_size_prefixed_chunks(payload) {
                decode_size_prefixed_none_or_lossless(header, &chunks, true, width, height)
            } else {
                decode_none_or_lossless(header, payload, true, width, height)
            }
        }
        DecodeType::None => decode_none_or_lossless(header, payload, false, width, height),
        DecodeType::Palette => decode_palette(header, payload, width, height),
        DecodeType::Unknown { tag } => Err(Deepmap2Error::UnsupportedDecodeType(tag)),
    }
}

fn decode_payload_with_state(
    header: &Deepmap2Header,
    payload: &[u8],
    width: usize,
    height: usize,
    prev_row: Option<Vec<i16>>,
) -> Deepmap2Result<(Vec<u8>, Option<Vec<i16>>)> {
    if let PixelFormat::Unknown { tag } = header.pixel_format {
        return Err(Deepmap2Error::UnsupportedPixelFormat(tag));
    }

    match header.decode_type {
        DecodeType::Default => {
            if let Some(chunks) = parse_size_prefixed_chunks(payload) {
                decode_size_prefixed_default_with_state(header, &chunks, width, height, prev_row)
            } else {
                decode_default_with_state(header, payload, width, height, prev_row)
            }
        }
        _ => Ok((decode_payload(header, payload, width, height)?, None)),
    }
}

/// 解码 payload 为 RGBA，使用 header 中的 width / height
pub fn decode_raw(header: &Deepmap2Header, payload: &[u8]) -> Deepmap2Result<Vec<u8>> {
    decode_raw_with_options(header, payload, None, None, None)
}

/// 解码 payload 为 RGBA，使用指定的输出高度（KCBC tile 路径）
pub fn decode_raw_with_height(
    header: &Deepmap2Header,
    payload: &[u8],
    output_height: u16,
) -> Deepmap2Result<Vec<u8>> {
    decode_raw_with_options(header, payload, None, Some(output_height), None)
}

pub fn decode_raw_with_options(
    header: &Deepmap2Header,
    payload: &[u8],
    output_width: Option<u16>,
    output_height: Option<u16>,
    pixel_format_override: Option<PixelFormat>,
) -> Deepmap2Result<Vec<u8>> {
    let header = apply_pixel_format_override(header, pixel_format_override);
    decode_payload(
        &header,
        payload,
        output_width.unwrap_or(header.width) as usize,
        output_height.unwrap_or(header.height) as usize,
    )
}

pub fn decode_raw_with_options_and_state(
    header: &Deepmap2Header,
    payload: &[u8],
    output_width: Option<u16>,
    output_height: Option<u16>,
    pixel_format_override: Option<PixelFormat>,
    prev_row: Option<Vec<i16>>,
) -> Deepmap2Result<(Vec<u8>, Option<Vec<i16>>)> {
    let header = apply_pixel_format_override(header, pixel_format_override);
    decode_payload_with_state(
        &header,
        payload,
        output_width.unwrap_or(header.width) as usize,
        output_height.unwrap_or(header.height) as usize,
        prev_row,
    )
}

#[cfg(test)]
mod tests {
    use super::decompress_lzfse;
    use lzfse_rust::{LzfseRingDecoder, LzfseRingEncoder};

    #[cfg(target_arch = "wasm32")]
    use crate::Deepmap2Error;

    fn encode_lzfse(data: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::new();
        let mut encoder = LzfseRingEncoder::default();
        encoder
            .encode_bytes(data, &mut encoded)
            .expect("test lzfse encode should succeed");
        encoded
    }

    fn build_palette4_size_prefixed_lzvn(width: u16, height: u16, indexes: &[u8]) -> Vec<u8> {
        assert_eq!(indexes.len(), width as usize * height as usize);

        let palette = [
            0xFFFF0000u32, // red
            0xFF00FF00u32, // green
            0xFF0000FFu32, // blue
        ];
        let compressed = lzvn::encode_raw(indexes);
        let mut data = Vec::new();
        data.extend_from_slice(b"dmp2");
        data.push(4); // Palette
        data.push(0);
        data.push(0);
        data.push(4); // Rgba8888
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(&height.to_le_bytes());
        data.extend_from_slice(&(palette.len() as u16).to_le_bytes());
        data.extend_from_slice(&4u16.to_le_bytes()); // palette type 4: index plane
        for entry in palette {
            data.extend_from_slice(&entry.to_le_bytes());
        }
        data.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        data.extend_from_slice(&compressed);
        data
    }

    #[test]
    fn single_apple_stream_keeps_pure_rust_decode_path() {
        let expected = b"single-stream-decode-regression";
        let encoded = encode_lzfse(expected);

        let decoded = decompress_lzfse(&encoded).expect("single Apple stream should decode");
        assert_eq!(decoded, expected);
    }

    #[test]
    fn concatenated_apple_streams_decode_in_pure_rust() {
        let first_raw = b"deepmap2 concatenated apple stream part one";
        let second_raw = b"deepmap2 concatenated apple stream part two";
        let first = encode_lzfse(first_raw);
        let second = encode_lzfse(second_raw);

        let mut concatenated = Vec::new();
        concatenated.extend_from_slice(&first);
        concatenated.extend_from_slice(&[0u8; 4]);
        concatenated.extend_from_slice(&second);

        let mut single_stream_output = Vec::new();
        let mut decoder = LzfseRingDecoder::default();
        assert!(
            decoder
                .decode_bytes(&concatenated, &mut single_stream_output)
                .is_err(),
            "single-stream decoder should reject concatenated Apple streams"
        );

        let decoded =
            decompress_lzfse(&concatenated).expect("concatenated Apple streams should decode");
        assert_eq!(decoded, [first_raw.as_ref(), second_raw.as_ref()].concat());
    }

    #[test]
    fn palette_type4_size_prefixed_lzvn_decodes() {
        let indexes = [0, 1, 2, 2, 1, 0];
        let data = build_palette4_size_prefixed_lzvn(3, 2, &indexes);

        let image = crate::decode(&data).expect("palette LZVN payload should decode");

        assert_eq!((image.width, image.height), (3, 2));
        assert_eq!(
            image.rgba,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 255, 0, 255,
                255, 0, 0, 255,
            ]
        );
    }

    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test;

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    fn invalid_non_apple_stream_reports_lzfse_decompress() {
        let err =
            decompress_lzfse(b"not-valid-lzfse").expect_err("invalid non-Apple stream should fail");
        assert!(matches!(err, Deepmap2Error::LzfseDecompress));
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    fn invalid_apple_stream_reports_native_fallback_unavailable() {
        let err = decompress_lzfse(b"bvx2not-valid-lzfse")
            .expect_err("invalid Apple compression stream should fail on wasm fallback path");
        assert!(matches!(err, Deepmap2Error::NativeFallbackUnavailable(_)));
    }
}
