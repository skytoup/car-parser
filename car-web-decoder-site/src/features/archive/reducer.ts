import type {
  BatchDownloadFailure,
  CarArchiveClient,
  DocumentInfo,
  ImageInfo,
  ImageListItem,
  LoadPhase,
} from "@/lib/types";

export type PreviewStatus = "idle" | "loading" | "ready" | "error";
export type SingleDownloadStatus = "idle" | "loading" | "error";
export type BatchStatus =
  | "idle"
  | "running"
  | "success"
  | "partial-failure"
  | "error";

export type PreviewView =
  | {
      kind: "none";
      reason: "empty" | "download-only" | "document";
      message: string;
    }
  | {
      kind: "img-binary";
      url: string;
      mimeType: string;
    }
  | {
      kind: "canvas-rgba";
      width: number;
      height: number;
      rgba: Uint8ClampedArray;
    }
  | {
      kind: "color-swatch";
      colorSpace: string;
      components: number[];
      cssColor: string;
    };

export interface ArchiveState {
  phase: LoadPhase;
  loadToken: number;
  fileName: string | null;
  client: CarArchiveClient | null;
  documentInfo: DocumentInfo | null;
  images: ImageListItem[];
  selectedEntryId: string | null;
  selectedEntryInfo: ImageInfo | null;
  detailError: string | null;
  loadError: string | null;
  preview: {
    status: PreviewStatus;
    entryId: string | null;
    view: PreviewView;
    error: string | null;
  };
  singleDownload: {
    status: SingleDownloadStatus;
    entryId: string | null;
    error: string | null;
  };
  batchDownload: {
    status: BatchStatus;
    completed: number;
    total: number;
    failures: BatchDownloadFailure[];
    error: string | null;
    archiveName: string | null;
  };
}

function emptyPreview(): ArchiveState["preview"] {
  return {
    status: "idle",
    entryId: null,
    view: {
      kind: "none",
      reason: "empty",
      message: "请选择一个资源以查看预览。",
    },
    error: null,
  };
}

function emptySingleDownload(): ArchiveState["singleDownload"] {
  return {
    status: "idle",
    entryId: null,
    error: null,
  };
}

function emptyBatchDownload(): ArchiveState["batchDownload"] {
  return {
    status: "idle",
    completed: 0,
    total: 0,
    failures: [],
    error: null,
    archiveName: null,
  };
}

function resetSelectionState(): Pick<
  ArchiveState,
  "selectedEntryId" | "selectedEntryInfo" | "detailError" | "preview" | "singleDownload"
> {
  return {
    selectedEntryId: null,
    selectedEntryInfo: null,
    detailError: null,
    preview: emptyPreview(),
    singleDownload: emptySingleDownload(),
  };
}

function resetProcessingState(): Pick<
  ArchiveState,
  "selectedEntryId" | "selectedEntryInfo" | "detailError" | "preview" | "singleDownload" | "batchDownload"
> {
  return {
    ...resetSelectionState(),
    batchDownload: emptyBatchDownload(),
  };
}

export function createInitialArchiveState(): ArchiveState {
  return {
    phase: "idle",
    loadToken: 0,
    fileName: null,
    client: null,
    documentInfo: null,
    images: [],
    loadError: null,
    ...resetProcessingState(),
  };
}

export type ArchiveAction =
  | { type: "load/start"; token: number; fileName: string }
  | { type: "load/phase"; token: number; phase: Exclude<LoadPhase, "ready" | "error"> }
  | {
      type: "load/success";
      token: number;
      client: CarArchiveClient;
      documentInfo: DocumentInfo;
      images: ImageListItem[];
    }
  | { type: "load/error"; token: number; error: string }
  | { type: "entry/select"; entryId: string }
  | { type: "entry/clear-selection" }
  | { type: "entry/info-ready"; token: number; entryId: string; info: ImageInfo }
  | { type: "entry/info-error"; token: number; entryId: string; error: string }
  | { type: "preview/loading"; token: number; entryId: string }
  | { type: "preview/ready"; token: number; entryId: string; view: PreviewView }
  | { type: "preview/error"; token: number; entryId: string; error: string }
  | { type: "download/single-start"; token: number; entryId: string }
  | { type: "download/single-success"; token: number; entryId: string }
  | { type: "download/single-error"; token: number; entryId: string; error: string }
  | { type: "download/batch-start"; token: number; total: number }
  | { type: "download/batch-progress"; token: number; completed: number }
  | {
      type: "download/batch-finish";
      token: number;
      failures: BatchDownloadFailure[];
      archiveName: string;
    }
  | {
      type: "download/batch-error";
      token: number;
      error: string;
      failures: BatchDownloadFailure[];
    };

function isSameSession(state: ArchiveState, token: number): boolean {
  return state.loadToken === token;
}

function isCurrentSelected(state: ArchiveState, entryId: string): boolean {
  return state.selectedEntryId === entryId;
}

export function archiveReducer(
  state: ArchiveState,
  action: ArchiveAction,
): ArchiveState {
  switch (action.type) {
    case "load/start":
      return {
        ...state,
        phase: "reading-file",
        loadToken: action.token,
        fileName: action.fileName,
        client: null,
        documentInfo: null,
        images: [],
        loadError: null,
        ...resetProcessingState(),
      };

    case "load/phase":
      if (!isSameSession(state, action.token)) {
        return state;
      }
      return {
        ...state,
        phase: action.phase,
      };

    case "load/success":
      if (!isSameSession(state, action.token)) {
        return state;
      }
      return {
        ...state,
        phase: "ready",
        client: action.client,
        documentInfo: action.documentInfo,
        images: action.images,
        loadError: null,
        ...resetProcessingState(),
      };

    case "load/error":
      if (!isSameSession(state, action.token)) {
        return state;
      }
      return {
        ...state,
        phase: "error",
        client: null,
        documentInfo: null,
        images: [],
        loadError: action.error,
        ...resetProcessingState(),
      };

    case "entry/select":
      return {
        ...state,
        selectedEntryId: action.entryId,
        selectedEntryInfo: null,
        detailError: null,
        preview: {
          status: "idle",
          entryId: action.entryId,
          view: {
            kind: "none",
            reason: "empty",
            message: "正在准备预览数据…",
          },
          error: null,
        },
        singleDownload: {
          status: "idle",
          entryId: action.entryId,
          error: null,
        },
      };

    case "entry/clear-selection":
      return {
        ...state,
        ...resetSelectionState(),
      };

    case "entry/info-ready":
      if (
        !isSameSession(state, action.token) ||
        !isCurrentSelected(state, action.entryId)
      ) {
        return state;
      }
      return {
        ...state,
        selectedEntryInfo: action.info,
        detailError: null,
      };

    case "entry/info-error":
      if (
        !isSameSession(state, action.token) ||
        !isCurrentSelected(state, action.entryId)
      ) {
        return state;
      }
      return {
        ...state,
        detailError: action.error,
      };

    case "preview/loading":
      if (
        !isSameSession(state, action.token) ||
        !isCurrentSelected(state, action.entryId)
      ) {
        return state;
      }
      return {
        ...state,
        preview: {
          ...state.preview,
          status: "loading",
          entryId: action.entryId,
          error: null,
        },
      };

    case "preview/ready":
      if (
        !isSameSession(state, action.token) ||
        !isCurrentSelected(state, action.entryId)
      ) {
        return state;
      }
      return {
        ...state,
        preview: {
          status: "ready",
          entryId: action.entryId,
          view: action.view,
          error: null,
        },
      };

    case "preview/error":
      if (
        !isSameSession(state, action.token) ||
        !isCurrentSelected(state, action.entryId)
      ) {
        return state;
      }
      return {
        ...state,
        preview: {
          ...state.preview,
          status: "error",
          entryId: action.entryId,
          error: action.error,
        },
      };

    case "download/single-start":
      if (
        !isSameSession(state, action.token) ||
        !isCurrentSelected(state, action.entryId)
      ) {
        return state;
      }
      return {
        ...state,
        singleDownload: {
          status: "loading",
          entryId: action.entryId,
          error: null,
        },
      };

    case "download/single-success":
      if (
        !isSameSession(state, action.token) ||
        !isCurrentSelected(state, action.entryId)
      ) {
        return state;
      }
      return {
        ...state,
        singleDownload: {
          status: "idle",
          entryId: action.entryId,
          error: null,
        },
      };

    case "download/single-error":
      if (
        !isSameSession(state, action.token) ||
        !isCurrentSelected(state, action.entryId)
      ) {
        return state;
      }
      return {
        ...state,
        singleDownload: {
          status: "error",
          entryId: action.entryId,
          error: action.error,
        },
      };

    case "download/batch-start":
      if (!isSameSession(state, action.token)) {
        return state;
      }
      return {
        ...state,
        batchDownload: {
          status: "running",
          completed: 0,
          total: action.total,
          failures: [],
          error: null,
          archiveName: null,
        },
      };

    case "download/batch-progress":
      if (!isSameSession(state, action.token)) {
        return state;
      }
      return {
        ...state,
        batchDownload: {
          ...state.batchDownload,
          completed: action.completed,
        },
      };

    case "download/batch-finish":
      if (!isSameSession(state, action.token)) {
        return state;
      }
      return {
        ...state,
        batchDownload: {
          status:
            action.failures.length > 0 ? "partial-failure" : "success",
          completed: state.batchDownload.total,
          total: state.batchDownload.total,
          failures: action.failures,
          error: null,
          archiveName: action.archiveName,
        },
      };

    case "download/batch-error":
      if (!isSameSession(state, action.token)) {
        return state;
      }
      return {
        ...state,
        batchDownload: {
          ...state.batchDownload,
          status: "error",
          failures: action.failures,
          error: action.error,
        },
      };

    default:
      return state;
  }
}
