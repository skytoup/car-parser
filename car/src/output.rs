use std::path::{Component, Path, PathBuf};

use crate::car::{CSIItem, Car};
use crate::model::Encoding;
use crate::model::rendition::Rendition;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputIdentityKind {
    CanonicalSourceOutput,
    VariantOutput,
}

#[derive(Debug, Clone)]
pub struct OutputIdentity<'a> {
    kind: OutputIdentityKind,
    facet_name: String,
    item: &'a CSIItem,
}

impl<'a> OutputIdentity<'a> {
    pub fn canonical_source(facet_name: impl Into<String>, item: &'a CSIItem) -> Self {
        Self {
            kind: OutputIdentityKind::CanonicalSourceOutput,
            facet_name: facet_name.into(),
            item,
        }
    }

    pub fn variant(facet_name: impl Into<String>, item: &'a CSIItem) -> Self {
        Self {
            kind: OutputIdentityKind::VariantOutput,
            facet_name: facet_name.into(),
            item,
        }
    }

    pub fn kind(&self) -> OutputIdentityKind {
        self.kind
    }

    pub fn facet_name(&self) -> &str {
        &self.facet_name
    }

    pub fn item(&self) -> &'a CSIItem {
        self.item
    }

    pub fn include_scale_suffix(&self) -> bool {
        matches!(self.kind, OutputIdentityKind::VariantOutput)
    }

    pub fn canonical_identity_key(&self) -> Option<Vec<u16>> {
        matches!(self.kind, OutputIdentityKind::CanonicalSourceOutput)
            .then(|| self.item.key_values().to_vec())
    }
}

impl Car {
    pub fn output_identity_for_payload<'a>(
        &'a self,
        logical_facet_name: &str,
        logical_item: &'a CSIItem,
        payload_item: &'a CSIItem,
    ) -> Option<OutputIdentity<'a>> {
        match supported_output_identity(payload_item)? {
            OutputIdentityKind::CanonicalSourceOutput => {
                let (facet_name, item) = self
                    .find_canonical_named_item(payload_item)
                    .unwrap_or((logical_facet_name.to_string(), payload_item));
                Some(OutputIdentity::canonical_source(facet_name, item))
            }
            OutputIdentityKind::VariantOutput => {
                Some(OutputIdentity::variant(logical_facet_name, logical_item))
            }
        }
    }

    fn find_canonical_named_item<'a>(
        &'a self,
        payload_item: &'a CSIItem,
    ) -> Option<(String, &'a CSIItem)> {
        let key_values = payload_item.key_values();
        for (facet_name, facet) in self.named_facets() {
            let Some(items) = self.items_for_facet(facet) else {
                continue;
            };
            if let Some(item) = items.iter().find(|item| {
                item.key_values() == key_values && supported_output_identity(item).is_some()
            }) {
                return Some((facet_name.to_string(), item));
            }
        }
        None
    }
}

pub fn supported_output_identity(item: &CSIItem) -> Option<OutputIdentityKind> {
    match &item.header().rendition {
        Some(Rendition::RawData(_))
            if matches!(
                item.header().encoding,
                Encoding::Data | Encoding::PDF | Encoding::SVG
            ) =>
        {
            Some(OutputIdentityKind::CanonicalSourceOutput)
        }
        Some(Rendition::Color(_))
        | Some(Rendition::ThemeCBCK(_))
        | Some(Rendition::MultisizeImageSet(_))
        | Some(Rendition::RawData(_)) => Some(OutputIdentityKind::VariantOutput),
        _ => None,
    }
}

pub fn rendition_scale(item: &CSIItem) -> u32 {
    item.attributes()
        .iter()
        .find(|attr| attr.name == crate::rendition::AttributeType::Scale && attr.val > 0)
        .map(|attr| u32::from(attr.val))
        .unwrap_or_else(|| item.scale())
}

pub fn suggested_file_name(identity: &OutputIdentity<'_>, extension: &str) -> Option<String> {
    let mut file_name = sanitize_file_name(identity.item().name())?;

    let scale = rendition_scale(identity.item());
    if identity.include_scale_suffix() && scale > 1 {
        let stem = file_name
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !stem.ends_with(&format!("@{}x", scale)) {
            let current_ext = file_name
                .extension()
                .map(|ext| ext.to_string_lossy().to_string());
            let renamed = match current_ext {
                Some(current_ext) => format!("{stem}@{scale}x.{current_ext}"),
                None => format!("{stem}@{scale}x"),
            };
            file_name = PathBuf::from(renamed);
        }
    }

    if !extension.is_empty() {
        let has_expected_extension = file_name
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension));
        if !has_expected_extension {
            file_name.set_extension(extension);
        }
    }

    file_name
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

pub fn default_raw_extension(encoding: Encoding) -> &'static str {
    match encoding {
        Encoding::HEIF => "heic",
        Encoding::PDF => "pdf",
        Encoding::SVG => "svg",
        Encoding::JPEG => "jpg",
        Encoding::WEBP => "webp",
        Encoding::Data | Encoding::None => "bin",
        _ => "bin",
    }
}

pub fn default_raw_extension_for_item(item: &CSIItem) -> &'static str {
    if crate::image_conv::is_hevc_compressed(item) {
        "heic"
    } else {
        default_raw_extension(item.header().encoding)
    }
}

pub(crate) fn sanitize_file_name(name: &str) -> Option<PathBuf> {
    let mut file_name = None;

    for component in Path::new(name).components() {
        match component {
            Component::Normal(part) => file_name = Some(PathBuf::from(part)),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    file_name
}
