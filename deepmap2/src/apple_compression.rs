#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

#[cfg(not(target_arch = "wasm32"))]
static RAW_TEMP_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum AppleCompressionError {
    #[error("Apple compression stream decompression failed: {0}")]
    DecodeFailed(String),
    #[error("Native fallback unavailable: {0}")]
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    NativeFallbackUnavailable(&'static str),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn is_apple_compression_stream(data: &[u8]) -> bool {
    matches!(
        data.get(..4),
        Some(b"bvx2") | Some(b"bvxn") | Some(b"bvx-") | Some(b"bvx$")
    )
}

pub fn decode_lzfse_with_fallback(data: &[u8]) -> Result<Vec<u8>, AppleCompressionError> {
    let cap = data.len().saturating_mul(16).max(4096);
    let mut out = Vec::with_capacity(cap);
    let mut decoder = lzfse_rust::LzfseRingDecoder::default();
    if let Ok(n) = decoder.decode_bytes(data, &mut out)
        && (n > 0 || data.is_empty())
    {
        out.truncate(n as usize);
        return Ok(out);
    }

    decode_lzfse_with_tool(data)
}

#[cfg(target_arch = "wasm32")]
fn decode_lzfse_with_tool(_data: &[u8]) -> Result<Vec<u8>, AppleCompressionError> {
    Err(AppleCompressionError::NativeFallbackUnavailable(
        "Apple compression stream decompression requires /usr/bin/compression_tool",
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_lzfse_with_tool(data: &[u8]) -> Result<Vec<u8>, AppleCompressionError> {
    let pid = std::process::id();
    let seq = RAW_TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
    let input_path = std::env::temp_dir().join(format!("carparser-raw-{pid}-{seq}.lzfse"));
    let output_path = input_path.with_extension("out");

    std::fs::write(&input_path, data)?;
    let result = Command::new("/usr/bin/compression_tool")
        .args([
            "-decode",
            "-a",
            "lzfse",
            "-i",
            input_path.to_str().unwrap_or_default(),
            "-o",
            output_path.to_str().unwrap_or_default(),
        ])
        .output();

    let decoded = match result {
        Ok(out) if out.status.success() => std::fs::read(&output_path)?,
        _ => {
            remove_temp_files(&input_path, &output_path);
            return Err(AppleCompressionError::DecodeFailed(
                "Apple compression stream decompression failed".to_string(),
            ));
        }
    };

    remove_temp_files(&input_path, &output_path);
    Ok(decoded)
}

#[cfg(not(target_arch = "wasm32"))]
fn remove_temp_files(input_path: &std::path::Path, output_path: &std::path::Path) {
    let _ = std::fs::remove_file(input_path);
    let _ = std::fs::remove_file(output_path);
}
