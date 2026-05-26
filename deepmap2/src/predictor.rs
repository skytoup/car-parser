/// 预测器组大小（对应 YCoCg 三分量: Y、Co、Cg）
pub const PREDICTOR_GROUP_SIZE: usize = 3;

/// 截断除以 2（向零截断，与 Python trunc_div2 一致）
#[inline]
fn trunc_div2(v: i32) -> i32 {
    v / 2
}

/// 将 i32 截断为 i16 范围（wrapping）
#[inline]
fn wrap_i16(v: i32) -> i16 {
    v as i16
}

/// 对行数据应用反预测器，返回重建后的行数据。
///
/// `predictor_raw` 为每行存储的预测器字节（Deepmap2Predictor 枚举值）:
/// 0=None, 1=Paeth, 2=Left, 3=Up, 4=Mean
///
/// `row` 为当前行的 i16 样本切片（长度 = width * split_stream_components）
/// `prev_row` 为上一行的重建结果（第一行为 None）
pub fn apply_predictor(predictor_raw: u8, row: &[i16], prev_row: Option<&[i16]>) -> Vec<i16> {
    apply_predictor_with_stride(predictor_raw, row, prev_row, PREDICTOR_GROUP_SIZE)
}

pub(crate) fn apply_predictor_with_stride(
    predictor_raw: u8,
    row: &[i16],
    prev_row: Option<&[i16]>,
    stride: usize,
) -> Vec<i16> {
    let count = row.len();
    match predictor_raw {
        0 => unpredict_none(row),
        1 => unpredict_paeth(row, prev_row, count, stride),
        2 => unpredict_left(row, count, stride),
        3 => unpredict_up(row, prev_row, count),
        4 => unpredict_mean(row, prev_row, count, stride),
        _ => row.to_vec(),
    }
}

/// None 预测器: 恒等变换
fn unpredict_none(data: &[i16]) -> Vec<i16> {
    data.to_vec()
}

/// Left 预测器: 以 PREDICTOR_GROUP_SIZE 为步长累加
///
/// 前 PREDICTOR_GROUP_SIZE 个样本直接输出，之后每个样本加上
/// 相同组偏移的前一个输出值（即同分量的左邻像素）。
fn unpredict_left(data: &[i16], count: usize, stride: usize) -> Vec<i16> {
    let mut output = vec![0i16; count];
    let head = stride.min(count);
    output[..head].copy_from_slice(&data[..head]);
    for i in stride..count {
        output[i] = wrap_i16(data[i] as i32 + output[i - stride] as i32);
    }
    output
}

/// Up 预测器: 每个样本加上上一行相同位置的值
fn unpredict_up(data: &[i16], prev_row: Option<&[i16]>, count: usize) -> Vec<i16> {
    let mut output = vec![0i16; count];
    for i in 0..count {
        let up = prev_row.map_or(0, |p| p[i] as i32);
        output[i] = wrap_i16(data[i] as i32 + up);
    }
    output
}

/// Mean 预测器: 预测值 = truncate((left + up + 1) / 2)
fn unpredict_mean(data: &[i16], prev_row: Option<&[i16]>, count: usize, stride: usize) -> Vec<i16> {
    let mut output = vec![0i16; count];
    // 前 stride 个样本用 Up 预测
    for i in 0..stride.min(count) {
        let up = prev_row.map_or(0, |p| p[i] as i32);
        output[i] = wrap_i16(data[i] as i32 + up);
    }
    for i in stride..count {
        let left = output[i - stride] as i32;
        let up = prev_row.map_or(0, |p| p[i] as i32);
        let predictor = trunc_div2(left + up + 1);
        output[i] = wrap_i16(data[i] as i32 + predictor);
    }
    output
}

/// vImage 风格的 Paeth 预测器（仅在 left / up 之间二选一）
fn paeth_predictor(left: i32, up: i32, up_left: i32) -> i32 {
    let dist_left = (up - up_left).abs();
    let dist_up = (left - up_left).abs();
    if dist_left <= dist_up { left } else { up }
}

/// Paeth 预测器（组粒度，每组 PREDICTOR_GROUP_SIZE 个分量）
fn unpredict_paeth(
    data: &[i16],
    prev_row: Option<&[i16]>,
    count: usize,
    stride: usize,
) -> Vec<i16> {
    let mut output = vec![0i16; count];
    // 前 stride 个样本用 Up 预测
    for i in 0..stride.min(count) {
        let up = prev_row.map_or(0, |p| p[i] as i32);
        output[i] = wrap_i16(data[i] as i32 + up);
    }
    let mut i = stride;
    while i < count {
        let group_size = stride.min(count - i);
        // 判断本组用 left 还是 up（以组内首个分量为判断基准）
        let left0 = output[i - stride] as i32;
        let up0 = prev_row.map_or(0, |p| p[i] as i32);
        let up_left0 = prev_row.map_or(0, |p| p[i - stride] as i32);
        let predicted_first = paeth_predictor(left0, up0, up_left0);
        let use_left = predicted_first == left0;

        for offset in 0..group_size {
            let left = output[i + offset - stride] as i32;
            let up = prev_row.map_or(0, |p| p[i + offset] as i32);
            let base = if use_left { left } else { up };
            output[i + offset] = wrap_i16(data[i + offset] as i32 + base);
        }
        i += PREDICTOR_GROUP_SIZE;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_predictor_is_identity() {
        let row = vec![1i16, -2, 300, 0, 100];
        let result = apply_predictor(0, &row, None);
        assert_eq!(result, row);
    }

    #[test]
    fn left_predictor_cumulative_sum() {
        // 对 RGB 分量（组大小=3）: 前3个直接输出，之后每个加上 3 步前的输出值
        // input:  [1, 2, 3,  4, 5, 6]
        // output: [1, 2, 3,  1+4=5, 2+5=7, 3+6=9]
        let row = vec![1i16, 2, 3, 4, 5, 6];
        let result = apply_predictor(2, &row, None);
        assert_eq!(result, vec![1, 2, 3, 5, 7, 9]);
    }

    #[test]
    fn up_predictor_adds_prev_row() {
        let row = vec![1i16, 2, 3];
        let prev = vec![10i16, 20, 30];
        let result = apply_predictor(3, &row, Some(&prev));
        assert_eq!(result, vec![11, 22, 33]);
    }

    #[test]
    fn up_predictor_no_prev_row_is_identity() {
        let row = vec![5i16, 10, 15];
        let result = apply_predictor(3, &row, None);
        assert_eq!(result, row);
    }

    #[test]
    fn mean_predictor_first_group_uses_up() {
        // 前3个用 up 预测，之后用 mean
        let row = vec![0i16, 0, 0, 2, 4, 6];
        let prev = vec![10i16, 20, 30, 40, 50, 60];
        let result = apply_predictor(4, &row, Some(&prev));
        // 前3: 0+10=10, 0+20=20, 0+30=30
        // i=3: left=10, up=40, mean=trunc((10+40+1)/2)=25, output=2+25=27
        // i=4: left=20, up=50, mean=trunc((20+50+1)/2)=35, output=4+35=39
        // i=5: left=30, up=60, mean=trunc((30+60+1)/2)=45, output=6+45=51
        assert_eq!(result, vec![10, 20, 30, 27, 39, 51]);
    }

    #[test]
    fn paeth_predictor_no_prev_row() {
        // 无前一行时等价于 left 预测
        let row = vec![1i16, 2, 3, 4, 5, 6];
        let result = apply_predictor(1, &row, None);
        // 前3直接输出: [1, 2, 3]
        // i=3: left0=1, up0=0, up_left0=0, paeth(1,0,0)=1(dist_left=0<=dist_up=1), use_left
        //   base=output[0]=1, output[3]=4+1=5
        // i=4: left0=2, up0=0, paeth=2, use_left, base=output[1]=2, output[4]=5+2=7
        // i=5: base=output[2]=3, output[5]=6+3=9
        assert_eq!(result, vec![1, 2, 3, 5, 7, 9]);
    }

    #[test]
    fn wrap_on_overflow() {
        // i16 overflow wraps correctly
        let row = vec![32767i16, 0, 0, 1, 0, 0];
        let result = apply_predictor(2, &row, None);
        // output[0]=32767, output[3]=wrap(32767+1)=-32768
        assert_eq!(result[3], i16::MIN);
    }
}
