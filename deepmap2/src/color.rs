use crate::model::PixelFormat;
use crate::predictor::PREDICTOR_GROUP_SIZE;

#[inline]
fn clamp_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// 截断除以 2（向零截断）
#[inline]
fn trunc_div2(v: i32) -> i32 {
    v / 2
}

/// YCoCg → RGB 转换
///
/// `scale`: chroma_scale（来自 header.version，非零则左移 1 位）
pub fn ycocg_to_rgb(y: i32, co: i32, cg: i32, scale: u8) -> (u8, u8, u8) {
    let co_scaled = co << scale;
    let cg_scaled = cg << scale;
    let co_half = trunc_div2(co_scaled);
    let cg_half = trunc_div2(cg_scaled);
    let temp = y - cg_half;
    let red = clamp_u8(temp + co_scaled - co_half);
    let green = clamp_u8(temp + cg_scaled);
    let blue = clamp_u8(temp - co_half);
    (red, green, blue)
}

/// 将一行解码后的 i16 样本（长度 = width * PREDICTOR_GROUP_SIZE）转换为 RGBA 字节
///
/// `alpha_row`: 若 pixel_format.has_alpha()，提供该行的 alpha 字节切片（长度=width）
/// `scale`: chroma_scale 值
pub fn row_to_rgba(
    pixel_format: PixelFormat,
    decoded_row: &[i16],
    alpha_row: Option<&[u8]>,
    scale: u8,
) -> Vec<u8> {
    let width = decoded_row.len() / PREDICTOR_GROUP_SIZE;
    let mut rgba = vec![0u8; width * 4];

    for px in 0..width {
        let sample_base = px * PREDICTOR_GROUP_SIZE;
        let rgba_base = px * 4;
        let luminance = decoded_row[sample_base];

        match pixel_format {
            PixelFormat::G8 => {
                let gray = luminance as u8;
                rgba[rgba_base..rgba_base + 4].copy_from_slice(&[gray, gray, gray, 0xFF]);
            }
            PixelFormat::GA88 => {
                let gray = luminance as u8;
                let alpha = alpha_row.map_or(0xFF, |a| a[px]);
                rgba[rgba_base..rgba_base + 4].copy_from_slice(&[gray, gray, gray, alpha]);
            }
            PixelFormat::Rgb888 => {
                let (red, green, blue) = ycocg_to_rgb(
                    luminance as i32,
                    decoded_row[sample_base + 1] as i32,
                    decoded_row[sample_base + 2] as i32,
                    scale,
                );
                rgba[rgba_base..rgba_base + 4].copy_from_slice(&[blue, green, red, 0xFF]);
            }
            PixelFormat::Rgba8888 => {
                let (red, green, blue) = ycocg_to_rgb(
                    luminance as i32,
                    decoded_row[sample_base + 1] as i32,
                    decoded_row[sample_base + 2] as i32,
                    scale,
                );
                let alpha = alpha_row.map_or(0xFF, |a| a[px]);
                rgba[rgba_base..rgba_base + 4].copy_from_slice(&[blue, green, red, alpha]);
            }
            PixelFormat::Unknown { .. } => {}
        }
    }
    rgba
}

/// 将 pixel_format 编码的原始字节直接转换为 RGBA（用于 None / Lossless 路径）
pub fn output_bytes_to_rgba(
    pixel_format: PixelFormat,
    width: usize,
    height: usize,
    bytes: &[u8],
) -> Vec<u8> {
    let pixel_count = width * height;
    let mut rgba = vec![0u8; pixel_count * 4];

    match pixel_format {
        PixelFormat::G8 => {
            for (i, &gray) in bytes[..pixel_count].iter().enumerate() {
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[gray, gray, gray, 0xFF]);
            }
        }
        PixelFormat::GA88 => {
            for i in 0..pixel_count {
                let gray = bytes[i * 2];
                let alpha = bytes[i * 2 + 1];
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[gray, gray, gray, alpha]);
            }
        }
        PixelFormat::Rgb888 => {
            for i in 0..pixel_count {
                let base = i * 3;
                let blue = bytes[base];
                let green = bytes[base + 1];
                let red = bytes[base + 2];
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[red, green, blue, 0xFF]);
            }
        }
        PixelFormat::Rgba8888 => {
            for i in 0..pixel_count {
                let base = i * 4;
                let blue = bytes[base];
                let green = bytes[base + 1];
                let red = bytes[base + 2];
                let alpha = bytes[base + 3];
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[red, green, blue, alpha]);
            }
        }
        PixelFormat::Unknown { .. } => {}
    }
    rgba
}

/// 调色板条目（u32 LE BGRA 编码）转 RGBA 字节
pub fn palette_entry_to_rgba(entry: u32, alpha_override: Option<u8>) -> [u8; 4] {
    let blue = (entry & 0xFF) as u8;
    let green = ((entry >> 8) & 0xFF) as u8;
    let red = ((entry >> 16) & 0xFF) as u8;
    let alpha = alpha_override.unwrap_or(((entry >> 24) & 0xFF) as u8);
    [red, green, blue, alpha]
}

/// 调色板索引数组 → RGBA（Type 4 路径）
///
/// `indices`: 每像素一个字节的调色板索引
/// `palette`: 调色板条目（u32 LE BGRA），长度 = palette_size
#[cfg(test)]
pub fn palette_to_rgba(indices: &[u8], palette: &[u32]) -> Vec<u8> {
    let mut rgba = vec![0u8; indices.len() * 4];
    for (i, &idx) in indices.iter().enumerate() {
        let entry = palette.get(idx as usize).copied().unwrap_or(0);
        let bytes = palette_entry_to_rgba(entry, None);
        rgba[i * 4..i * 4 + 4].copy_from_slice(&bytes);
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ycocg_round_trip() {
        // Y=128, Co=0, Cg=0 → temp=128, R=clamp(128), G=clamp(128), B=clamp(128)
        let (r, g, b) = ycocg_to_rgb(128, 0, 0, 0);
        assert_eq!((r, g, b), (128, 128, 128));
    }

    #[test]
    fn ycocg_red_channel() {
        // 纯红色: Y=85, Co=128, Cg=-85 (after YCoCg forward)
        // 直接测已知输入->输出
        let (r, g, b) = ycocg_to_rgb(85, 128, -85, 0);
        // temp = 85 - trunc(-85/2) = 85 - (-42) = 127
        // R = clamp(127 + 128 - 64) = clamp(191)
        // G = clamp(127 + (-85)) = clamp(42)
        // B = clamp(127 - 64) = clamp(63)
        assert_eq!(r, 191);
        assert_eq!(g, 42);
        assert_eq!(b, 63);
    }

    #[test]
    fn palette_lookup() {
        // entry 0 = 0x00FF0000 (blue=0, green=0, red=255, alpha=0) → RGBA=(255,0,0,0)
        let palette = vec![0x000000FFu32, 0x0000FF00u32];
        let indices = vec![0u8, 1, 0];
        let rgba = palette_to_rgba(&indices, &palette);
        // entry 0: blue=0xFF, green=0, red=0, alpha=0 → [red=0, green=0, blue=255, alpha=0]
        assert_eq!(&rgba[0..4], &[0, 0, 255, 0]);
        // entry 1: blue=0, green=0xFF, red=0, alpha=0 → [0, 255, 0, 0]
        assert_eq!(&rgba[4..8], &[0, 255, 0, 0]);
    }
}
