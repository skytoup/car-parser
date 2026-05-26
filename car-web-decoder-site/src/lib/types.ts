export type LoadPhase =
  | "idle"
  | "reading-file"
  | "loading-archive"
  | "listing-assets"
  | "ready"
  | "error";

export type EntryKind = "image" | "document" | "raw-data" | "color";

export type PreviewStrategy =
  | "img-binary"
  | "canvas-rgba"
  | "document"
  | "download-only"
  | "color-swatch";

export type DownloadStrategy = "original" | "png" | "none";

export type SelectionReason =
  | "original-browser-binary"
  | "decoded-raster"
  | "download-only-original"
  | "metadata-color";

export type DocumentInfoValue = string | number | boolean | null;

export type DocumentInfo = Record<string, DocumentInfoValue>;

export interface EntryInfo {
  id: string;
  preview_source_id: string | null;
  facet_name: string;
  rendition_name: string;
  width: number;
  height: number;
  scale: number;
  logical_layout: string;
  resolved_encoding: string;
  entry_kind: EntryKind;
  preview_strategy: PreviewStrategy;
  download_strategy: DownloadStrategy;
  suggested_extension: string;
  suggested_file_name: string;
  mime_type: string;
  preserves_original_format: boolean;
  selection_reason: SelectionReason;
  downloadable: boolean;
  color_space?: string | null;
  color_components?: number[] | null;
  css_color?: string | null;
}

export type ImageInfo = EntryInfo;

export interface EntryListItem {
  id: string;
  preview_source_id: string | null;
  facet_name: string;
  rendition_name: string;
  width: number;
  height: number;
  scale: number;
  resolved_encoding: string;
  entry_kind: EntryKind;
  preview_strategy: PreviewStrategy;
  downloadable: boolean;
  css_color?: string | null;
}

export type ImageListItem = EntryListItem;

export interface DisplayPayloadImgBinary {
  preview_strategy: "img-binary";
  mime_type: string;
  suggested_extension: string;
  suggested_file_name: string;
  preserves_original_format: boolean;
  selection_reason: SelectionReason;
  bytes: Uint8Array | number[];
}

export interface DisplayPayloadCanvasRgba {
  preview_strategy: "canvas-rgba";
  width: number;
  height: number;
  suggested_extension: string;
  suggested_file_name: string;
  preserves_original_format: boolean;
  selection_reason: SelectionReason;
  rgba: Uint8Array | number[];
}

export interface DisplayPayloadDocument {
  preview_strategy: "document";
  mime_type: string;
  suggested_extension: string;
  suggested_file_name: string;
  preserves_original_format: boolean;
  selection_reason: SelectionReason;
  bytes: Uint8Array | number[];
}

export interface DisplayPayloadDownloadOnly {
  preview_strategy: "download-only";
  mime_type: string;
  suggested_extension: string;
  suggested_file_name: string;
  preserves_original_format: boolean;
  selection_reason: SelectionReason;
}

export interface DisplayPayloadColorSwatch {
  preview_strategy: "color-swatch";
  color_space: string;
  components: number[];
  css_color: string;
}

export type DisplayPayload =
  | DisplayPayloadImgBinary
  | DisplayPayloadCanvasRgba
  | DisplayPayloadDocument
  | DisplayPayloadDownloadOnly
  | DisplayPayloadColorSwatch;

export interface DownloadPayload {
  bytes: Uint8Array | number[];
  mime_type: string;
  suggested_extension: string;
  suggested_file_name: string;
  download_strategy: DownloadStrategy;
  preserves_original_format: boolean;
  selection_reason: SelectionReason;
}

export interface ThumbnailPayloadImgBinary {
  preview_strategy: "img-binary";
  mime_type: string;
  bytes: Uint8Array | number[];
}

export interface ThumbnailPayloadDownloadOnly {
  preview_strategy: "download-only";
}

export type ThumbnailPayload =
  | ThumbnailPayloadImgBinary
  | ThumbnailPayloadDownloadOnly;

export interface BatchDownloadFailure {
  id: string;
  fileName: string;
  reason: string;
}

export interface CarArchiveClient {
  documentInfo(): Promise<DocumentInfo>;
  listEntries(): Promise<EntryInfo[]>;
  listImages(): Promise<ImageInfo[]>;
  listEntrySummaries(): Promise<EntryListItem[]>;
  listImageSummaries(): Promise<ImageListItem[]>;
  getEntryInfo(id: string): Promise<EntryInfo>;
  getImageInfo(id: string): Promise<ImageInfo>;
  getDisplayPayload(id: string): Promise<DisplayPayload>;
  getDownloadPayload(id: string): Promise<DownloadPayload>;
  getThumbnailPayload(
    id: string,
    options?: { maxDimension?: number },
  ): Promise<ThumbnailPayload>;
  terminate(): void;
}

export interface CarArchiveClientCtor {
  load(
    bytes: Uint8Array | ArrayBuffer | ArrayBufferView,
    options?: unknown,
  ): Promise<CarArchiveClient>;
}
