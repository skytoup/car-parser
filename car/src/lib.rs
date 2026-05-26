//! Stable high-level parser for Apple `.car` asset catalogs.
//!
//! Most callers should start with [`Car`] and [`CSIItem`]. The crate root
//! exposes the stable document and rendition surface, while [`raw`] contains
//! binary-layout models for tooling or low-level inspection.
//!
//! When the `image` feature is enabled, [`image`] exposes free functions for
//! decoding or writing image assets without reaching into parser internals.

pub mod asset;
mod car;
pub mod decode;
pub mod diagnostics;
pub mod export;
#[cfg(feature = "image")]
pub mod image;
mod image_conv;
pub mod metadata;
mod model;
pub mod output;
pub mod raw;
pub mod view;

pub use bom::ByteSlice as PayloadBytes;

pub use crate::asset::{
    AssetEntry, AssetId, AssetKind, AssetVariant, AttributeSet, PayloadKind,
    ResolvedPayloadSummary, TypedAttribute, VariantAttributes, VariantMatchError, VariantQuery,
    asset_kind_for_item, payload_kind_for_item,
};
pub use crate::car::{
    CSIItem, Car, CarError, CarResult, FacetItem, ReferenceRect, ReferenceResolveError,
    ResolvedInternalReference,
};
pub use crate::decode::{
    DecodeBudgetError, DecodeOptions, DecodeStage, OrientationPolicy, RenditionData,
    RenditionDataRef,
};
pub use crate::diagnostics::{
    DiagnosticIssue, DiagnosticsReport, DiagnosticsTotals, EntryDiagnostics, UnsupportedReason,
};
pub use crate::image_conv::RgbaPayload;
pub use crate::metadata::{
    ExifOrientation, TlvMetadata, TlvMetric, TlvMetrics, TlvRect, TlvReference, UnknownTlv,
};
pub use crate::model::rendition;
pub use crate::model::{
    CSIHeader, CSIMetadata, ColorModel, ColorSpace, Encoding, ExtendedMetadata, Header,
};
pub use crate::output::{
    OutputIdentity, OutputIdentityKind, default_raw_extension, default_raw_extension_for_item,
    rendition_scale, suggested_file_name, supported_output_identity,
};
