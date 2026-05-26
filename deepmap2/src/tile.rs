use std::collections::BTreeMap;

use crate::model::PixelFormat;
use crate::{DecodedImage, Deepmap2Error, Deepmap2Result};

/// KCBC 块信息
struct KcbcBlock<'a> {
    #[allow(dead_code)]
    offset: usize,
    /// 0x04 字段：列索引（tile 在网格中的列号，从 0 开始）
    #[allow(dead_code)]
    col_index: u32,
    /// 0x08 字段：行索引（tile 在网格中的行号，从 0 开始）
    #[allow(dead_code)]
    row_index: u32,
    /// row_hint：该块实际输出的行数（从 offset+0x0C 读取）
    row_hint: u32,
    /// 该块包含的 dmp2 原始数据（从 dmp2 magic 到下一个块开始或数据末尾）
    dmp2_data: &'a [u8],
}

/// 扫描 data 中所有 "KCBC" magic，返回块信息列表。
///
/// 如果发现 KCBC magic 但头部不完整（截断），返回错误。
fn parse_kcbc_blocks(data: &[u8]) -> Deepmap2Result<Vec<KcbcBlock<'_>>> {
    let mut blocks = Vec::new();
    let mut search_offset = 0;

    while let Some(rel) = find_bytes(&data[search_offset..], b"KCBC") {
        let block_offset = search_offset + rel;
        if block_offset + 20 > data.len() {
            return Err(Deepmap2Error::InvalidData(
                "KCBC 块头被截断：数据不足 20 字节".to_string(),
            ));
        }

        let col_index = u32::from_le_bytes(
            data[block_offset + 0x04..block_offset + 0x08]
                .try_into()
                .unwrap(),
        );
        let row_index = u32::from_le_bytes(
            data[block_offset + 0x08..block_offset + 0x0C]
                .try_into()
                .unwrap(),
        );
        let row_hint = u32::from_le_bytes(
            data[block_offset + 0x0C..block_offset + 0x10]
                .try_into()
                .unwrap(),
        );
        let block_length = u32::from_le_bytes(
            data[block_offset + 0x10..block_offset + 0x14]
                .try_into()
                .unwrap(),
        ) as usize;

        // 确定块的数据范围
        let block_end = if block_length >= 0x20 && block_offset + block_length <= data.len() {
            block_offset + block_length
        } else {
            // 找到下一个 KCBC 或使用剩余全部数据
            find_bytes(&data[block_offset + 4..], b"KCBC")
                .map(|r| block_offset + 4 + r)
                .unwrap_or(data.len())
        };

        let block_span = &data[block_offset..block_end];

        // 在块范围内查找 dmp2 magic（缺失则报错，不静默跳过）
        let dmp2_rel = find_bytes(block_span, b"dmp2")
            .ok_or_else(|| Deepmap2Error::InvalidData("KCBC 块中未找到 dmp2 头部".to_string()))?;
        let dmp2_data = &block_span[dmp2_rel..];
        blocks.push(KcbcBlock {
            offset: block_offset,
            col_index,
            row_index,
            row_hint,
            dmp2_data,
        });

        search_offset = block_end;
    }

    Ok(blocks)
}

/// 在切片中搜索子序列，返回第一次出现的偏移量
pub fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 从包含 KCBC 块的 data 中解析各块内嵌的 dmp2 数据切片
///
/// 返回每个 tile 的原始 dmp2 字节（可直接传给 `decode()`）
pub fn parse_kcbc(data: &[u8]) -> Deepmap2Result<Vec<Vec<u8>>> {
    let blocks = parse_kcbc_blocks(data)?;
    if blocks.is_empty() {
        return Err(Deepmap2Error::InvalidData(
            "输入数据中未找到 KCBC 块".to_string(),
        ));
    }
    Ok(blocks.into_iter().map(|b| b.dmp2_data.to_vec()).collect())
}

/// 解码 KCBC 序列，按 row_index/col_index 还原 tile 网格。
pub fn decode_kcbc_sequence(data: &[u8]) -> Deepmap2Result<DecodedImage> {
    decode_kcbc_sequence_with_options(data, None, None, None)
}

pub fn decode_kcbc_sequence_with_options(
    data: &[u8],
    expected_width: Option<u16>,
    expected_height: Option<u16>,
    pixel_format_override: Option<PixelFormat>,
) -> Deepmap2Result<DecodedImage> {
    use crate::raw::Deepmap2Header;
    use deku::prelude::*;

    let blocks = parse_kcbc_blocks(data)?;
    if blocks.is_empty() {
        return Err(Deepmap2Error::InvalidData(
            "输入数据中未找到 KCBC 块".to_string(),
        ));
    }

    struct TileInfo {
        col_index: u32,
        row_index: u32,
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        header: Deepmap2Header,
    }

    let mut tiles = Vec::with_capacity(blocks.len());
    let mut column_widths: BTreeMap<u32, usize> = BTreeMap::new();
    let mut row_heights: BTreeMap<u32, usize> = BTreeMap::new();

    for block in &blocks {
        let (tw, th, rgba, header) =
            decode_tile(block.dmp2_data, block.row_hint, pixel_format_override)?;
        let width = tw as usize;
        let height = th as usize;
        column_widths
            .entry(block.col_index)
            .and_modify(|existing| *existing = (*existing).max(width))
            .or_insert(width);
        row_heights
            .entry(block.row_index)
            .and_modify(|existing| *existing = (*existing).max(height))
            .or_insert(height);

        tiles.push(TileInfo {
            col_index: block.col_index,
            row_index: block.row_index,
            width: tw as usize,
            height: th as usize,
            rgba,
            header,
        });
    }

    let mut x_offsets = BTreeMap::new();
    let mut total_width = 0usize;
    for (&col_index, &width) in &column_widths {
        x_offsets.insert(col_index, total_width);
        total_width += width;
    }

    let mut y_offsets = BTreeMap::new();
    let mut total_height = 0usize;
    for (&row_index, &height) in &row_heights {
        y_offsets.insert(row_index, total_height);
        total_height += height;
    }

    if let Some(expected_width) = expected_width
        && total_width != expected_width as usize
    {
        return Err(Deepmap2Error::InvalidData(format!(
            "KCBC 输出宽度不一致: 期望 {}, 实际 {}",
            expected_width, total_width
        )));
    }
    if let Some(expected_height) = expected_height
        && total_height != expected_height as usize
    {
        return Err(Deepmap2Error::InvalidData(format!(
            "KCBC 输出高度不一致: 期望 {}, 实际 {}",
            expected_height, total_height
        )));
    }

    let mut rgba = vec![0u8; total_width * total_height * 4];
    for tile in &tiles {
        let Some(&dst_x) = x_offsets.get(&tile.col_index) else {
            return Err(Deepmap2Error::InvalidData(format!(
                "KCBC 缺少列偏移: col_index={}",
                tile.col_index
            )));
        };
        let Some(&dst_y) = y_offsets.get(&tile.row_index) else {
            return Err(Deepmap2Error::InvalidData(format!(
                "KCBC 缺少行偏移: row_index={}",
                tile.row_index
            )));
        };

        for row in 0..tile.height {
            let src_start = row * tile.width * 4;
            let src_end = src_start + tile.width * 4;
            let dst_start = ((dst_y + row) * total_width + dst_x) * 4;
            let dst_end = dst_start + tile.width * 4;
            rgba[dst_start..dst_end].copy_from_slice(&tile.rgba[src_start..src_end]);
        }
    }

    let mut cursor = std::io::Cursor::new(blocks[0].dmp2_data);
    let mut reader = Reader::new(&mut cursor);
    let source_header = tiles
        .first()
        .map(|tile| tile.header.clone())
        .unwrap_or_else(|| Deepmap2Header::from_reader_with_ctx(&mut reader, ()).unwrap());

    Ok(DecodedImage {
        source_header: source_header.clone(),
        width: total_width as u16,
        height: total_height as u16,
        pixel_format: pixel_format_override.unwrap_or(source_header.pixel_format),
        rgba,
    })
}

/// 解码单个 tile 的 dmp2 数据，row_hint=0 时使用 header.height
fn decode_tile(
    dmp2_data: &[u8],
    row_hint: u32,
    pixel_format_override: Option<PixelFormat>,
) -> Deepmap2Result<(u16, u16, Vec<u8>, crate::model::Deepmap2Header)> {
    use crate::raw::Deepmap2Header;
    use deku::prelude::*;

    let mut cursor = std::io::Cursor::new(dmp2_data);
    let mut reader = Reader::new(&mut cursor);
    let header = Deepmap2Header::from_reader_with_ctx(&mut reader, ())?;
    let header_size = header.header_size();
    let payload = dmp2_data
        .get(header_size..)
        .ok_or(Deepmap2Error::Truncated)?;

    let output_height = if row_hint > 0 {
        u16::try_from(row_hint).map_err(|_| {
            Deepmap2Error::InvalidData(format!("KCBC row_hint 超出 u16 范围: {row_hint}"))
        })?
    } else {
        header.height
    };

    let output_width = header.width;
    let rgba = crate::codec::decode_raw_with_options(
        &header,
        payload,
        Some(output_width),
        Some(output_height),
        pixel_format_override,
    )?;

    let mut header = header;
    header.width = output_width;
    header.height = output_height;
    if let Some(pixel_format) = pixel_format_override {
        header.pixel_format = pixel_format;
    }

    Ok((output_width, output_height, rgba, header))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小 dmp2 数据（Type 1=None, G8, 无压缩）
    fn build_dmp2_none_g8(width: u16, height: u16, pixels: &[u8]) -> Vec<u8> {
        assert_eq!(pixels.len(), width as usize * height as usize);
        let mut data = Vec::new();
        data.extend_from_slice(b"dmp2");
        data.push(0x01); // decode_type = None
        data.push(0x00); // version
        data.push(0x00); // predictor_type
        data.push(0x01); // pixel_format = G8
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(&height.to_le_bytes());
        data.extend_from_slice(pixels);
        data
    }

    /// 构造 KCBC 容器，tiles: &[(row, col, row_hint, dmp2_data)]
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

    #[test]
    fn decode_kcbc_single_tile_matches_decode() {
        let pixels = [10u8, 20, 30, 40];
        let dmp2 = build_dmp2_none_g8(2, 2, &pixels);

        let kcbc = build_kcbc(&[(0, 0, 2, &dmp2)]);
        let kcbc_img = crate::decode_kcbc(&kcbc).unwrap();
        let raw_img = crate::decode(&dmp2).unwrap();

        assert_eq!(kcbc_img.width, raw_img.width);
        assert_eq!(kcbc_img.height, raw_img.height);
        assert_eq!(kcbc_img.rgba, raw_img.rgba);
    }

    #[test]
    fn decode_kcbc_multi_band_vertical_stitch() {
        let top = build_dmp2_none_g8(2, 2, &[1, 2, 3, 4]);
        let middle = build_dmp2_none_g8(2, 1, &[5, 6]);
        let bottom = build_dmp2_none_g8(2, 2, &[7, 8, 9, 10]);

        let kcbc = build_kcbc(&[(0, 0, 2, &top), (1, 0, 1, &middle), (2, 0, 2, &bottom)]);

        let img = crate::decode_kcbc(&kcbc).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 5);

        let px = |gray: u8| -> [u8; 4] { [gray, gray, gray, 0xFF] };
        let expected: Vec<u8> = [
            px(1),
            px(2),
            px(3),
            px(4),
            px(5),
            px(6),
            px(7),
            px(8),
            px(9),
            px(10),
        ]
        .concat();
        assert_eq!(img.rgba, expected);
    }

    #[test]
    fn decode_kcbc_grid_tiles_respect_row_and_col_indices() {
        let top_left = build_dmp2_none_g8(1, 1, &[1]);
        let top_right = build_dmp2_none_g8(1, 1, &[2]);
        let bottom_left = build_dmp2_none_g8(1, 1, &[3]);
        let bottom_right = build_dmp2_none_g8(1, 1, &[4]);

        let kcbc = build_kcbc(&[
            (0, 0, 1, &top_left),
            (0, 1, 1, &top_right),
            (1, 0, 1, &bottom_left),
            (1, 1, 1, &bottom_right),
        ]);

        let img = decode_kcbc_sequence_with_options(&kcbc, Some(2), Some(2), None).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);

        let px = |gray: u8| -> [u8; 4] { [gray, gray, gray, 0xFF] };
        let expected: Vec<u8> = [px(1), px(2), px(3), px(4)].concat();
        assert_eq!(img.rgba, expected);
    }

    #[test]
    fn decode_tile_rejects_row_hint_exceeding_u16() {
        let dmp2 = build_dmp2_none_g8(2, 2, &[0; 4]);
        let kcbc = build_kcbc(&[(0, 0, 65536, &dmp2)]); // u16::MAX + 1
        let result = crate::decode_kcbc(&kcbc);
        let err = result.err().expect("should error on oversized row_hint");
        let msg = format!("{err}");
        assert!(msg.contains("u16"), "should mention u16 overflow: {msg}");
    }

    #[test]
    fn parse_kcbc_empty_data_returns_error() {
        let result = parse_kcbc(b"no kcbc here");
        assert!(result.is_err());
    }

    #[test]
    fn parse_kcbc_truncated_header_returns_error() {
        // KCBC magic 后面只有 10 字节（不足 16 字节头部数据），应返回错误
        let mut data = b"KCBC".to_vec();
        data.extend_from_slice(&[0u8; 10]);
        let result = parse_kcbc(&data);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("截断"),
            "error should mention truncation: {err_msg}"
        );
    }

    #[test]
    fn parse_kcbc_truncated_second_block_returns_error() {
        // 第一个完整 KCBC 块 + 第二个截断的 KCBC 块
        let mut data = Vec::new();
        // Block 1: KCBC header (20 bytes) + "dmp2" magic + some payload
        data.extend_from_slice(b"KCBC");
        data.extend_from_slice(&0u32.to_le_bytes()); // col_index
        data.extend_from_slice(&0u32.to_le_bytes()); // row_index
        data.extend_from_slice(&10u32.to_le_bytes()); // row_hint
        data.extend_from_slice(&0u32.to_le_bytes()); // block_length=0 (will use next KCBC)
        data.extend_from_slice(b"dmp2");
        data.extend_from_slice(&[0u8; 8]); // some dmp2 data
        // Block 2: truncated KCBC (only magic + 8 bytes, need 16 more)
        data.extend_from_slice(b"KCBC");
        data.extend_from_slice(&[0u8; 8]); // only 8 bytes, need 16

        let result = parse_kcbc(&data);
        assert!(result.is_err(), "should error on truncated second block");
    }

    #[test]
    fn find_bytes_found() {
        assert_eq!(find_bytes(b"hello world", b"world"), Some(6));
    }

    #[test]
    fn find_bytes_not_found() {
        assert_eq!(find_bytes(b"hello", b"xyz"), None);
    }
}
