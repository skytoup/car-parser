//! Decoder for Apple's Deepmap2 image payloads.
//!
//! Use [`decode`] for standalone `dmp2` payloads and [`decode_kcbc`] for tiled
//! `KCBC` sequences. The stable root exposes decode entry points plus a small
//! enum surface, while [`raw`] contains the parsed binary header structs.

mod apple_compression;
pub mod codec;
mod color;
mod model;
mod predictor;
pub mod raw;
pub mod tile;

pub use crate::model::{DecodeType, PixelFormat, Predictor};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Deepmap2Error {
    #[error("invalid magic bytes")]
    InvalidMagic,
    #[error("truncated data")]
    Truncated,
    #[error("unsupported decode type: {0}")]
    UnsupportedDecodeType(u8),
    #[error("unsupported pixel format: {0}")]
    UnsupportedPixelFormat(u8),
    #[error("invalid palette size: {0}")]
    InvalidPaletteSize(u16),
    #[error("invalid palette type: {0}")]
    InvalidPaletteType(u16),
    #[error("invalid data: {0}")]
    InvalidData(String),
    #[error("lzfse decompression failed")]
    LzfseDecompress,
    #[error("native fallback unavailable: {0}")]
    NativeFallbackUnavailable(&'static str),
    #[error("lzvn decompression failed")]
    LzvnDecompress,
    #[error("deku parse error: {0}")]
    DekuParse(#[from] deku::DekuError),
}

pub type Deepmap2Result<T> = Result<T, Deepmap2Error>;

pub struct DecodedImage {
    pub source_header: raw::Deepmap2Header,
    pub width: u16,
    pub height: u16,
    pub pixel_format: PixelFormat,
    pub rgba: Vec<u8>,
}

pub fn decode(data: &[u8]) -> Deepmap2Result<DecodedImage> {
    decode_with_options(data, None, None, None)
}

pub fn decode_with_options(
    data: &[u8],
    output_width: Option<u16>,
    output_height: Option<u16>,
    pixel_format_override: Option<PixelFormat>,
) -> Deepmap2Result<DecodedImage> {
    use deku::prelude::*;

    let mut cursor = std::io::Cursor::new(data);
    let mut reader = Reader::new(&mut cursor);
    let header = raw::Deepmap2Header::from_reader_with_ctx(&mut reader, ())?;
    let header_size = header.header_size();
    let payload = data.get(header_size..).ok_or(Deepmap2Error::Truncated)?;

    let width = output_width.unwrap_or(header.width);
    let height = output_height.unwrap_or(header.height);
    let rgba = codec::decode_raw_with_options(
        &header,
        payload,
        Some(width),
        Some(height),
        pixel_format_override,
    )?;
    let pixel_format = pixel_format_override.unwrap_or(header.pixel_format);

    Ok(DecodedImage {
        source_header: header,
        width,
        height,
        pixel_format,
        rgba,
    })
}

pub fn decode_kcbc(data: &[u8]) -> Deepmap2Result<DecodedImage> {
    decode_kcbc_with_options(data, None, None, None)
}

pub fn decode_kcbc_with_options(
    data: &[u8],
    expected_width: Option<u16>,
    expected_height: Option<u16>,
    pixel_format_override: Option<PixelFormat>,
) -> Deepmap2Result<DecodedImage> {
    tile::decode_kcbc_sequence_with_options(
        data,
        expected_width,
        expected_height,
        pixel_format_override,
    )
}
