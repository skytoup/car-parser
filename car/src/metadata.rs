//! Stable typed views over CSI TLV metadata.
//!
//! The raw TLV structs remain available through [`crate::raw`] for reverse
//! engineering. This module keeps the common metadata surface small and owned so
//! callers do not need to depend on binary-layout structs.

use crate::car::CSIItem;
use crate::model::rendition::{self, RenditionType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExifOrientation {
    None,
    Normal,
    Mirrored,
    Rotated180,
    Rotated180Mirrored,
    Rotated90,
    Rotated90Mirrored,
    Rotated270,
    Rotated270Mirrored,
    Unknown(u32),
}

impl ExifOrientation {
    pub fn from_raw(value: &rendition::EXIFOrientationValue) -> Self {
        // Keep the stable metadata view aligned with standard EXIF numeric
        // orientation semantics. The raw enum variant names for ids 5-8 are
        // legacy labels and do not by themselves describe the full transform.
        match value {
            rendition::EXIFOrientationValue::None => Self::None,
            rendition::EXIFOrientationValue::Normal => Self::Normal,
            rendition::EXIFOrientationValue::Mirrored => Self::Mirrored,
            rendition::EXIFOrientationValue::Rotated180 => Self::Rotated180,
            rendition::EXIFOrientationValue::Rotated180Mirrored => Self::Rotated180Mirrored,
            rendition::EXIFOrientationValue::Rotated90 => Self::Rotated90Mirrored,
            rendition::EXIFOrientationValue::Rotated90Mirrored => Self::Rotated90,
            rendition::EXIFOrientationValue::Rotated270 => Self::Rotated270Mirrored,
            rendition::EXIFOrientationValue::Rotated270Mirrored => Self::Rotated270,
            rendition::EXIFOrientationValue::Unknown { tag } => Self::Unknown(*tag),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlvRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlvMetric {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlvMetrics {
    pub top_right_inset: TlvMetric,
    pub bottom_left_inset: TlvMetric,
    pub image_size: TlvMetric,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlvReference {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub layout: u16,
    pub key_byte_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTlv {
    pub tag: u32,
    pub byte_len: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlvMetadata {
    pub slices: Vec<TlvRect>,
    pub metrics: Option<TlvMetrics>,
    pub uti: Option<String>,
    pub exif_orientation: Option<ExifOrientation>,
    pub bytes_per_row: Option<u32>,
    pub reference: Option<TlvReference>,
    pub unknown: Vec<UnknownTlv>,
}

impl CSIItem {
    pub fn tlv_metadata(&self) -> TlvMetadata {
        let mut metadata = TlvMetadata::default();

        for tlv in &self.header().tlv_data {
            match tlv {
                RenditionType::Slices(slice) => {
                    metadata
                        .slices
                        .extend(slice.data.iter().map(|slice| TlvRect {
                            x: slice.x,
                            y: slice.y,
                            width: slice.width,
                            height: slice.height,
                        }));
                }
                RenditionType::Metrics(metrics) => {
                    metadata.metrics = Some(TlvMetrics {
                        top_right_inset: TlvMetric {
                            width: metrics.top_right_inset.width,
                            height: metrics.top_right_inset.height,
                        },
                        bottom_left_inset: TlvMetric {
                            width: metrics.bottom_left_inset.width,
                            height: metrics.bottom_left_inset.height,
                        },
                        image_size: TlvMetric {
                            width: metrics.image_size.width,
                            height: metrics.image_size.height,
                        },
                    });
                }
                RenditionType::UTI(uti) => {
                    metadata.uti = Some(
                        String::from_utf8_lossy(&uti.string)
                            .trim_end_matches('\0')
                            .to_string(),
                    );
                }
                RenditionType::EXIFOrientation(orientation) => {
                    metadata.exif_orientation =
                        Some(ExifOrientation::from_raw(&orientation.orientation));
                }
                RenditionType::BytesPerRow { bytes_per_row, .. } => {
                    metadata.bytes_per_row = Some(*bytes_per_row);
                }
                RenditionType::Reference(reference) => {
                    metadata.reference = Some(TlvReference {
                        x: reference.x,
                        y: reference.y,
                        width: reference.width,
                        height: reference.height,
                        layout: reference.layout,
                        key_byte_len: reference.keys.len(),
                    });
                }
                RenditionType::Unknown { tag, data } => {
                    metadata.unknown.push(UnknownTlv {
                        tag: *tag,
                        byte_len: data.data.len(),
                    });
                }
                RenditionType::BlendModeAndOpacity(_) => {}
            }
        }

        metadata
    }

    pub fn exif_orientation(&self) -> Option<ExifOrientation> {
        self.header().tlv_data.iter().find_map(|tlv| match tlv {
            RenditionType::EXIFOrientation(orientation) => {
                Some(ExifOrientation::from_raw(&orientation.orientation))
            }
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ExifOrientation;
    use crate::model::rendition::EXIFOrientationValue;

    #[test]
    fn exif_orientation_from_raw_maps_standard_exif_5_to_8() {
        assert_eq!(
            ExifOrientation::from_raw(&EXIFOrientationValue::Rotated90),
            ExifOrientation::Rotated90Mirrored
        );
        assert_eq!(
            ExifOrientation::from_raw(&EXIFOrientationValue::Rotated90Mirrored),
            ExifOrientation::Rotated90
        );
        assert_eq!(
            ExifOrientation::from_raw(&EXIFOrientationValue::Rotated270),
            ExifOrientation::Rotated270Mirrored
        );
        assert_eq!(
            ExifOrientation::from_raw(&EXIFOrientationValue::Rotated270Mirrored),
            ExifOrientation::Rotated270
        );
    }
}
