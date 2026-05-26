//! Archive diagnostics for unsupported, unknown, and unresolved data.

use crate::asset::{AssetKind, asset_kind_for_item};
use crate::car::{CSIItem, Car, ReferenceResolveError};
use crate::metadata::{TlvMetadata, UnknownTlv};
use crate::model::rendition::{CompressionType, LayoutType, Rendition, RenditionKind};
use crate::model::{ColorModel, Encoding};
use crate::output;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsReport {
    pub totals: DiagnosticsTotals,
    pub entries: Vec<EntryDiagnostics>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticsTotals {
    pub facets: usize,
    pub entries: usize,
    pub supported_outputs: usize,
    pub unsupported_outputs: usize,
    pub internal_references: usize,
    pub unresolved_references: usize,
    pub unknown_renditions: usize,
    pub unknown_tlvs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryDiagnostics {
    pub facet_name: String,
    pub rendition_name: String,
    pub key_values: Vec<u16>,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub layout: LayoutType,
    pub encoding: Encoding,
    pub color_model: ColorModel,
    pub rendition_kind: RenditionKind,
    pub compression: Option<CompressionType>,
    pub asset_kind: AssetKind,
    pub tlv: TlvMetadata,
    pub issues: Vec<DiagnosticIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticIssue {
    Unsupported(UnsupportedReason),
    UnknownRendition { tag: [u8; 4], byte_len: usize },
    UnknownTlv(UnknownTlv),
    UnresolvedReference(ReferenceResolveError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedReason {
    Layout(LayoutType),
    RenditionKind(RenditionKind),
    Encoding(Encoding),
    Compression(CompressionType),
}

impl Car {
    pub fn diagnostics(&self) -> DiagnosticsReport {
        let mut totals = DiagnosticsTotals::default();
        let mut entries = Vec::new();

        for (facet_name, facet) in self.named_facets() {
            totals.facets += 1;
            let Some(items) = self.items_for_facet(facet) else {
                continue;
            };
            for item in items {
                totals.entries += 1;
                let diagnostics = self.entry_diagnostics(facet_name, item);
                if diagnostics
                    .issues
                    .iter()
                    .any(|issue| matches!(issue, DiagnosticIssue::Unsupported(_)))
                {
                    totals.unsupported_outputs += 1;
                } else {
                    totals.supported_outputs += 1;
                }
                if matches!(item.layout(), LayoutType::InternalReference) {
                    totals.internal_references += 1;
                }
                for issue in &diagnostics.issues {
                    match issue {
                        DiagnosticIssue::UnknownRendition { .. } => totals.unknown_renditions += 1,
                        DiagnosticIssue::UnknownTlv(_) => totals.unknown_tlvs += 1,
                        DiagnosticIssue::UnresolvedReference(_) => {
                            totals.unresolved_references += 1
                        }
                        DiagnosticIssue::Unsupported(_) => {}
                    }
                }
                entries.push(diagnostics);
            }
        }

        DiagnosticsReport { totals, entries }
    }

    fn entry_diagnostics(&self, facet_name: &str, item: &CSIItem) -> EntryDiagnostics {
        let tlv = item.tlv_metadata();
        let mut issues = Vec::new();

        if let Some(reason) = unsupported_reason(item) {
            issues.push(DiagnosticIssue::Unsupported(reason));
        }

        if let Some(Rendition::Unknown { tag, data }) = item.header().rendition.as_ref() {
            issues.push(DiagnosticIssue::UnknownRendition {
                tag: *tag,
                byte_len: data.raw_data.len(),
            });
        }

        issues.extend(tlv.unknown.iter().cloned().map(DiagnosticIssue::UnknownTlv));

        if matches!(item.layout(), LayoutType::InternalReference)
            && let Err(error) = self.try_resolve_internal_reference(item)
        {
            issues.push(DiagnosticIssue::UnresolvedReference(error));
        }

        EntryDiagnostics {
            facet_name: facet_name.to_string(),
            rendition_name: item.name().to_string(),
            key_values: item.key_values().to_vec(),
            width: item.width(),
            height: item.height(),
            scale: item.scale(),
            layout: item.layout(),
            encoding: item.encoding(),
            color_model: item.color_model(),
            rendition_kind: item.rendition_kind(),
            compression: item.compression(),
            asset_kind: asset_kind_for_item(item),
            tlv,
            issues,
        }
    }
}

impl CSIItem {
    pub fn unsupported_reason(&self) -> Option<UnsupportedReason> {
        unsupported_reason(self)
    }
}

fn unsupported_reason(item: &CSIItem) -> Option<UnsupportedReason> {
    if matches!(item.layout(), LayoutType::InternalReference) {
        return None;
    }

    match item.header().rendition.as_ref() {
        Some(Rendition::Unknown { .. }) => {
            Some(UnsupportedReason::RenditionKind(RenditionKind::Unknown))
        }
        None => Some(UnsupportedReason::RenditionKind(RenditionKind::None)),
        Some(Rendition::ThemeCBCK(cbck)) if unsupported_compression(cbck.compression_type) => {
            Some(UnsupportedReason::Compression(cbck.compression_type))
        }
        Some(_) if matches!(item.encoding(), Encoding::Unknown { .. } | Encoding::None) => {
            Some(UnsupportedReason::Encoding(item.encoding()))
        }
        Some(_) if output::supported_output_identity(item).is_none() => {
            Some(UnsupportedReason::Layout(item.layout()))
        }
        _ => None,
    }
}

fn unsupported_compression(compression: CompressionType) -> bool {
    !matches!(
        compression,
        CompressionType::Uncompressed
            | CompressionType::Zip
            | CompressionType::Lzfse
            | CompressionType::Lzvn
            | CompressionType::HEVC
            | CompressionType::Deepmap2
    )
}

#[cfg(test)]
mod tests {
    use test_support::fixture_path;

    use super::*;

    #[test]
    fn promptbar_hevc_is_supported_as_original_output() {
        let car = Car::new(fixture_path("Assets.car")).expect("load smoke fixture");
        let report = car.diagnostics();
        let promptbar = report
            .entries
            .iter()
            .filter(|entry| entry.facet_name == "PromptBarBkg")
            .collect::<Vec<_>>();

        assert_eq!(promptbar.len(), 4);
        assert!(promptbar.iter().all(|entry| {
            !entry.issues.iter().any(|issue| {
                matches!(
                    issue,
                    DiagnosticIssue::Unsupported(UnsupportedReason::Compression(
                        CompressionType::HEVC
                    ))
                )
            })
        }));
    }
}
