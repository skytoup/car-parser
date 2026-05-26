use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    Image,
    Document,
    RawData,
    Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreviewStrategy {
    ImgBinary,
    CanvasRgba,
    Document,
    DownloadOnly,
    ColorSwatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DownloadStrategy {
    Original,
    Png,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelectionReason {
    OriginalBrowserBinary,
    DecodedRaster,
    DownloadOnlyOriginal,
    MetadataColor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInfo {
    pub id: String,
    pub preview_source_id: Option<String>,
    pub facet_name: String,
    pub rendition_name: String,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub logical_layout: String,
    pub resolved_encoding: String,
    pub entry_kind: EntryKind,
    pub preview_strategy: PreviewStrategy,
    pub download_strategy: DownloadStrategy,
    pub suggested_extension: String,
    pub suggested_file_name: String,
    pub mime_type: String,
    pub preserves_original_format: bool,
    pub selection_reason: SelectionReason,
    pub downloadable: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub color_space: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub color_components: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub css_color: Option<String>,
}

pub type ImageInfo = EntryInfo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryListItem {
    pub id: String,
    pub preview_source_id: Option<String>,
    pub facet_name: String,
    pub rendition_name: String,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub resolved_encoding: String,
    pub entry_kind: EntryKind,
    pub preview_strategy: PreviewStrategy,
    pub downloadable: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub css_color: Option<String>,
}

pub type ImageListItem = EntryListItem;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "preview_strategy", rename_all = "kebab-case")]
pub enum DisplayPayload {
    ImgBinary {
        mime_type: String,
        suggested_extension: String,
        suggested_file_name: String,
        preserves_original_format: bool,
        selection_reason: SelectionReason,
        bytes: Vec<u8>,
    },
    CanvasRgba {
        width: u32,
        height: u32,
        suggested_extension: String,
        suggested_file_name: String,
        preserves_original_format: bool,
        selection_reason: SelectionReason,
        rgba: Vec<u8>,
    },
    Document {
        mime_type: String,
        suggested_extension: String,
        suggested_file_name: String,
        preserves_original_format: bool,
        selection_reason: SelectionReason,
        bytes: Vec<u8>,
    },
    DownloadOnly {
        mime_type: String,
        suggested_extension: String,
        suggested_file_name: String,
        preserves_original_format: bool,
        selection_reason: SelectionReason,
    },
    ColorSwatch {
        color_space: String,
        components: Vec<f64>,
        css_color: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadPayload {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub suggested_extension: String,
    pub suggested_file_name: String,
    pub download_strategy: DownloadStrategy,
    pub preserves_original_format: bool,
    pub selection_reason: SelectionReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSummary {
    pub facets: usize,
    pub entries: usize,
    pub supported_outputs: usize,
    pub unsupported_outputs: usize,
    pub internal_references: usize,
    pub unresolved_references: usize,
    pub unknown_renditions: usize,
    pub unknown_tlvs: usize,
    pub issue_samples: Vec<DiagnosticIssueSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticIssueSummary {
    pub facet_name: String,
    pub rendition_name: String,
    pub issue: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "preview_strategy", rename_all = "kebab-case")]
pub enum ThumbnailPayload {
    ImgBinary { mime_type: String, bytes: Vec<u8> },
    DownloadOnly,
}

impl EntryInfo {
    pub fn list_item(&self) -> EntryListItem {
        EntryListItem {
            id: self.id.clone(),
            preview_source_id: self.preview_source_id.clone(),
            facet_name: self.facet_name.clone(),
            rendition_name: self.rendition_name.clone(),
            width: self.width,
            height: self.height,
            scale: self.scale,
            resolved_encoding: self.resolved_encoding.clone(),
            entry_kind: self.entry_kind,
            preview_strategy: self.preview_strategy,
            downloadable: self.downloadable,
            css_color: self.css_color.clone(),
        }
    }
}
