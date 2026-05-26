use std::fmt;

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    ArchiveLoadFailed,
    EntryNotFound,
    EntryNotDownloadable,
    NativeFallbackUnavailable,
    UnsupportedEncoding,
    UnresolvedReference,
    DecodeFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct WasmError {
    code: ErrorCode,
    message: String,
}

pub type WasmResult<T> = Result<T, WasmError>;

impl WasmError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn archive_load(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ArchiveLoadFailed, message)
    }

    pub fn entry_not_found(id: &str) -> Self {
        Self::new(
            ErrorCode::EntryNotFound,
            format!("entry `{id}` does not exist in this archive"),
        )
    }

    pub fn entry_not_downloadable(id: &str) -> Self {
        Self::new(
            ErrorCode::EntryNotDownloadable,
            format!("entry `{id}` is metadata-only and cannot be downloaded"),
        )
    }

    pub fn unresolved_reference(name: &str) -> Self {
        Self::new(
            ErrorCode::UnresolvedReference,
            format!("failed to resolve InternalReference payload for `{name}`"),
        )
    }

    pub fn unresolved_reference_with_reason(name: &str, reason: impl std::fmt::Display) -> Self {
        Self::new(
            ErrorCode::UnresolvedReference,
            format!("failed to resolve InternalReference payload for `{name}`: {reason}"),
        )
    }

    pub fn decode_failed(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::DecodeFailed, message)
    }

    pub fn unsupported_encoding(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::UnsupportedEncoding, message)
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn to_payload(&self) -> ErrorPayload {
        ErrorPayload {
            code: self.code,
            message: self.message.clone(),
        }
    }

    pub fn to_js_value(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.to_payload())
            .unwrap_or_else(|_| JsValue::from_str(self.message()))
    }
}

impl fmt::Display for WasmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", error_code_name(self.code), self.message)
    }
}

impl std::error::Error for WasmError {}

impl From<WasmError> for JsValue {
    fn from(value: WasmError) -> Self {
        value.to_js_value()
    }
}

impl From<&WasmError> for JsValue {
    fn from(value: &WasmError) -> Self {
        value.to_js_value()
    }
}

impl From<car::CarError> for WasmError {
    fn from(value: car::CarError) -> Self {
        use car::CarError;

        match value {
            CarError::UnsupportedEncoding(encoding) => {
                Self::unsupported_encoding(format!("unsupported encoding {encoding:?}"))
            }
            CarError::UnsupportedCompression(compression) => Self::decode_failed(format!(
                "unsupported compression {compression:?} on pure-Rust wasm path"
            )),
            CarError::NativeFallbackUnavailable(message) => {
                Self::new(ErrorCode::NativeFallbackUnavailable, message)
            }
            CarError::ReferenceResolve(error) => {
                Self::new(ErrorCode::UnresolvedReference, error.to_string())
            }
            CarError::DecodeBudgetExceeded(error) => Self::decode_failed(error.to_string()),
            CarError::Deepmap2(deepmap2::Deepmap2Error::NativeFallbackUnavailable(message)) => {
                Self::new(ErrorCode::NativeFallbackUnavailable, message)
            }
            CarError::DecodeFailed(message) => Self::decode_failed(message),
            other => Self::archive_load(other.to_string()),
        }
    }
}

fn error_code_name(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::ArchiveLoadFailed => "ArchiveLoadFailed",
        ErrorCode::EntryNotFound => "EntryNotFound",
        ErrorCode::EntryNotDownloadable => "EntryNotDownloadable",
        ErrorCode::NativeFallbackUnavailable => "NativeFallbackUnavailable",
        ErrorCode::UnsupportedEncoding => "UnsupportedEncoding",
        ErrorCode::UnresolvedReference => "UnresolvedReference",
        ErrorCode::DecodeFailed => "DecodeFailed",
    }
}
