let archive = null;
let wasmArchiveCtor = null;
let wasmArchiveCtorPromise = null;
let parentPort = null;
const nodeWorkerPromise =
  typeof process !== "undefined" && process.versions?.node
    ? import("node:worker_threads")
        .then((nodeWorker) => {
          parentPort = nodeWorker?.parentPort ?? null;
          if (parentPort) {
            parentPort.on("message", (data) => void handleMessage({ data }));
          }
          return nodeWorker;
        })
        .catch(() => null)
    : Promise.resolve(null);

function postResult(message) {
  if (parentPort) {
    parentPort.postMessage(message);
    return;
  }
  globalThis.postMessage(message);
}

function postDiagnostic(requestId, requestType, stage, detail) {
  const prefix = `[car-wasm/worker] ${requestType}#${requestId} ${stage}`;
  if (stage === "request-failed") {
    if (detail == null) {
      console.error(prefix);
    } else {
      console.error(prefix, detail);
    }
  } else if (detail == null) {
    console.debug(prefix);
  } else {
    console.debug(prefix, detail);
  }

  postResult({
    kind: "diagnostic",
    diagnostic: {
      requestId,
      requestType,
      source: "worker",
      stage,
      timestamp: Date.now(),
      ...(detail == null ? {} : { detail }),
    },
  });
}

function normalizeError(error) {
  if (error && typeof error === "object" && "code" in error && "message" in error) {
    return {
      code: String(error.code),
      message: String(error.message),
      ...(error.phase == null ? {} : { phase: String(error.phase) }),
      ...(error.details == null ? {} : { details: error.details }),
    };
  }
  if (error instanceof Error) {
    return { code: "Unknown", message: error.message };
  }
  return { code: "Unknown", message: String(error) };
}

async function loadWasmArchiveCtor(requestId, requestType) {
  if (wasmArchiveCtor) {
    postDiagnostic(
      requestId,
      requestType,
      "wasm-module-ready",
      "reusing loaded wasm-bindgen module",
    );
    return wasmArchiveCtor;
  }

  if (!wasmArchiveCtorPromise) {
    postDiagnostic(
      requestId,
      requestType,
      "wasm-module-load-start",
      "loading generated wasm-bindgen module",
    );
    wasmArchiveCtorPromise = import("../pkg/car_wasm.js")
      .then((module) => {
        wasmArchiveCtor = module.WasmArchive;
        postDiagnostic(
          requestId,
          requestType,
          "wasm-module-load-complete",
          "generated wasm-bindgen module ready",
        );
        return wasmArchiveCtor;
      })
      .catch((error) => {
        wasmArchiveCtor = null;
        wasmArchiveCtorPromise = null;
        throw error;
      });
  } else {
    postDiagnostic(
      requestId,
      requestType,
      "wasm-module-load-reuse",
      "reusing in-flight wasm-bindgen module load",
    );
  }

  return wasmArchiveCtorPromise;
}

function requireArchive() {
  if (!archive) {
    throw {
      code: "ArchiveNotLoaded",
      message: "car-wasm archive is not loaded in worker",
    };
  }
  return archive;
}

async function handleMessage(event) {
  const { requestId, type, payload } = event.data;
  let phase = "request-received";
  postDiagnostic(requestId, type, phase);

  try {
    let result;
    switch (type) {
      case "load": {
        const byteLength = payload?.bytes?.byteLength ?? payload?.bytes?.length ?? 0;
        const WasmArchive = await loadWasmArchiveCtor(requestId, type);
        phase = "archive-load-start";
        postDiagnostic(requestId, type, phase, `byteLength=${byteLength}`);
        archive = WasmArchive.fromBytes(payload.bytes);
        phase = "archive-load-complete";
        postDiagnostic(requestId, type, phase, `byteLength=${byteLength}`);
        result = null;
        break;
      }
      case "document_info":
        phase = "document-info-start";
        postDiagnostic(requestId, type, phase);
        result = requireArchive().documentInfo();
        phase = "document-info-complete";
        postDiagnostic(requestId, type, phase);
        break;
      case "list_entries":
        phase = "list-entries-start";
        postDiagnostic(requestId, type, phase);
        result = requireArchive().listEntries();
        phase = "list-entries-complete";
        postDiagnostic(requestId, type, phase);
        break;
      case "list_images":
        phase = "list-images-start";
        postDiagnostic(requestId, type, phase);
        result = requireArchive().listImages();
        phase = "list-images-complete";
        postDiagnostic(requestId, type, phase);
        break;
      case "list_entry_summaries":
        phase = "list-entry-summaries-start";
        postDiagnostic(requestId, type, phase);
        result = requireArchive().listEntrySummaries();
        phase = "list-entry-summaries-complete";
        postDiagnostic(requestId, type, phase);
        break;
      case "list_image_summaries":
        phase = "list-image-summaries-start";
        postDiagnostic(requestId, type, phase);
        result = requireArchive().listImageSummaries();
        phase = "list-image-summaries-complete";
        postDiagnostic(requestId, type, phase);
        break;
      case "get_entry_info":
        phase = "get-entry-info-start";
        postDiagnostic(requestId, type, phase, payload?.id ?? null);
        result = requireArchive().getEntryInfo(payload.id);
        phase = "get-entry-info-complete";
        postDiagnostic(requestId, type, phase, payload?.id ?? null);
        break;
      case "get_image_info":
        phase = "get-image-info-start";
        postDiagnostic(requestId, type, phase, payload?.id ?? null);
        result = requireArchive().getImageInfo(payload.id);
        phase = "get-image-info-complete";
        postDiagnostic(requestId, type, phase, payload?.id ?? null);
        break;
      case "get_display_payload":
        phase = "get-display-payload-start";
        postDiagnostic(requestId, type, phase, payload?.id ?? null);
        result = requireArchive().getDisplayPayload(payload.id);
        phase = "get-display-payload-complete";
        postDiagnostic(requestId, type, phase, payload?.id ?? null);
        break;
      case "get_download_payload":
        phase = "get-download-payload-start";
        postDiagnostic(requestId, type, phase, payload?.id ?? null);
        result = requireArchive().getDownloadPayload(payload.id);
        phase = "get-download-payload-complete";
        postDiagnostic(requestId, type, phase, payload?.id ?? null);
        break;
      case "get_thumbnail_payload":
        phase = "get-thumbnail-payload-start";
        postDiagnostic(requestId, type, phase, payload?.id ?? null);
        result = requireArchive().getThumbnailPayload(
          payload.id,
          payload?.options ?? undefined,
        );
        phase = "get-thumbnail-payload-complete";
        postDiagnostic(requestId, type, phase, payload?.id ?? null);
        break;
      default:
        throw new Error(`unknown car-wasm worker request: ${type}`);
    }

    postResult({ kind: "response", requestId, ok: true, result });
  } catch (error) {
    postDiagnostic(
      requestId,
      type,
      "request-failed",
      error?.message ?? String(error),
    );
    postResult({
      kind: "response",
      requestId,
      ok: false,
      error: {
        ...normalizeError(error),
        phase,
      },
    });
  }
}

if (typeof process === "undefined" || !process.versions?.node) {
  globalThis.onmessage = (event) => void handleMessage(event);
}
