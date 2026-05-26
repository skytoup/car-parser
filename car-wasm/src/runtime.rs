use serde::Deserialize;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::archive::ArchiveRuntime;
use crate::error::WasmError;

#[wasm_bindgen]
pub struct WasmArchive {
    inner: ArchiveRuntime,
}

#[wasm_bindgen]
impl WasmArchive {
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(bytes: Box<[u8]>) -> Result<WasmArchive, JsValue> {
        let archive = ArchiveRuntime::load(bytes.into_vec()).map_err(JsValue::from)?;
        Ok(Self { inner: archive })
    }

    #[wasm_bindgen(js_name = documentInfo)]
    pub fn document_info(&self) -> Result<JsValue, JsValue> {
        into_js(&self.inner.document_info())
    }

    #[wasm_bindgen(js_name = diagnosticsSummary)]
    pub fn diagnostics_summary(&self) -> Result<JsValue, JsValue> {
        into_js(&self.inner.diagnostics_summary())
    }

    #[wasm_bindgen(js_name = listEntries)]
    pub fn list_entries(&self) -> Result<JsValue, JsValue> {
        into_js(&self.inner.list_entries())
    }

    #[wasm_bindgen(js_name = listImages)]
    pub fn list_images(&self) -> Result<JsValue, JsValue> {
        into_js(&self.inner.list_images())
    }

    #[wasm_bindgen(js_name = listEntrySummaries)]
    pub fn list_entry_summaries(&self) -> Result<JsValue, JsValue> {
        into_js(&self.inner.list_entry_summaries())
    }

    #[wasm_bindgen(js_name = listImageSummaries)]
    pub fn list_image_summaries(&self) -> Result<JsValue, JsValue> {
        into_js(&self.inner.list_image_summaries())
    }

    #[wasm_bindgen(js_name = getEntryInfo)]
    pub fn get_entry_info(&self, id: &str) -> Result<JsValue, JsValue> {
        let info = self.inner.get_entry_info(id).map_err(JsValue::from)?;
        into_js(&info)
    }

    #[wasm_bindgen(js_name = getImageInfo)]
    pub fn get_image_info(&self, id: &str) -> Result<JsValue, JsValue> {
        let info = self.inner.get_image_info(id).map_err(JsValue::from)?;
        into_js(&info)
    }

    #[wasm_bindgen(js_name = getDisplayPayload)]
    pub fn get_display_payload(&self, id: &str) -> Result<JsValue, JsValue> {
        let payload = self.inner.get_display_payload(id).map_err(JsValue::from)?;
        into_js(&payload)
    }

    #[wasm_bindgen(js_name = getDownloadPayload)]
    pub fn get_download_payload(&self, id: &str) -> Result<JsValue, JsValue> {
        let payload = self.inner.get_download_payload(id).map_err(JsValue::from)?;
        into_js(&payload)
    }

    #[wasm_bindgen(js_name = getThumbnailPayload)]
    pub fn get_thumbnail_payload(&self, id: &str, options: JsValue) -> Result<JsValue, JsValue> {
        let options = parse_thumbnail_options(options)?;
        let payload = self
            .inner
            .get_thumbnail_payload(id, options.max_dimension)
            .map_err(JsValue::from)?;
        into_js(&payload)
    }
}

fn into_js<T>(value: &T) -> Result<JsValue, JsValue>
where
    T: Serialize,
{
    serde_wasm_bindgen::to_value(value)
        .map_err(|err| JsValue::from(WasmError::decode_failed(err.to_string())))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThumbnailOptions {
    max_dimension: Option<u32>,
}

fn parse_thumbnail_options(value: JsValue) -> Result<ThumbnailOptions, JsValue> {
    if value.is_undefined() || value.is_null() {
        return Ok(ThumbnailOptions::default());
    }

    serde_wasm_bindgen::from_value(value)
        .map_err(|err| JsValue::from(WasmError::decode_failed(err.to_string())))
}
