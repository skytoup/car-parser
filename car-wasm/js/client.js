export class CarWasmClientError extends Error {
  constructor(code, message, options = {}) {
    super(message);
    this.name = "CarWasmClientError";
    this.code = code;
    if (options.requestId != null) {
      this.requestId = options.requestId;
    }
    if (options.requestType != null) {
      this.requestType = options.requestType;
    }
    if (options.phase != null) {
      this.phase = options.phase;
    }
    if (Array.isArray(options.trace)) {
      this.trace = options.trace;
    }
    if (options.details != null) {
      this.details = options.details;
    }
    if (options.cause != null) {
      this.cause = options.cause;
    }
  }
}

const DEFAULT_REQUEST_TIMEOUT_MS = 30_000;
const DEFAULT_LOAD_TIMEOUT_MS = 110_000;

function defaultTimeoutMsFor(type) {
  return type === "load" ? DEFAULT_LOAD_TIMEOUT_MS : DEFAULT_REQUEST_TIMEOUT_MS;
}

function timestampNow() {
  return Date.now();
}

function formatTrace(trace) {
  if (!Array.isArray(trace) || trace.length === 0) {
    return "";
  }
  return trace
    .map((entry) => `${entry.source}:${entry.stage}`)
    .join(" -> ");
}

function formatDiagnosticDetail(detail) {
  if (detail == null) {
    return "";
  }
  if (typeof detail === "string") {
    return detail;
  }
  try {
    return JSON.stringify(detail);
  } catch {
    return String(detail);
  }
}

export class CarWasmArchiveClient {
  #worker;
  #pending = new Map();
  #nextRequestId = 0;
  #diagnosticListener = null;
  #timeoutMs = null;
  #consoleDiagnostics = true;

  static async load(bytes, options = {}) {
    const ownsWorker = options.worker == null;
    const worker =
      options.worker ??
      new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
    const client = new CarWasmArchiveClient(worker, options);
    let normalized =
      bytes instanceof Uint8Array
        ? bytes
        : bytes instanceof ArrayBuffer
          ? new Uint8Array(bytes)
          : ArrayBuffer.isView(bytes)
            ? new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength)
            : bytes;
    const transferable =
      normalized instanceof Uint8Array
        ? normalized.byteOffset === 0 &&
          normalized.byteLength === normalized.buffer.byteLength
          ? normalized
          : new Uint8Array(normalized)
        : normalized;
    try {
      await client.#request(
        "load",
        { bytes: transferable },
        transferable instanceof Uint8Array ? [transferable.buffer] : [],
      );
      return client;
    } catch (error) {
      if (ownsWorker) {
        try {
          client.terminate();
        } catch {}
      }
      throw error;
    }
  }

  constructor(worker, options = {}) {
    this.#worker = worker;
    this.#diagnosticListener =
      typeof options.onDiagnostic === "function" ? options.onDiagnostic : null;
    this.#timeoutMs =
      options.timeoutMs === 0
        ? 0
        : Number.isFinite(options.timeoutMs) && options.timeoutMs > 0
          ? Number(options.timeoutMs)
          : null;
    this.#consoleDiagnostics = options.consoleDiagnostics !== false;
    this.#worker.onmessage = (event) => {
      const message = event?.data ?? event;
      if (message?.kind === "diagnostic") {
        this.#handleDiagnostic(message.diagnostic ?? message);
        return;
      }

      const payload = message?.kind === "response" ? message : message ?? {};
      const { requestId, ok, result, error } = payload;
      const pending = this.#pending.get(requestId);
      if (!pending) {
        return;
      }
      this.#recordTrace(pending, "client", ok ? "response-ok" : "response-error");
      this.#clearTimer(pending);
      this.#pending.delete(requestId);
      if (ok) {
        pending.resolve(result);
        return;
      }
      pending.reject(
        this.#createError(
          pending,
          error?.code ?? "Unknown",
          error?.message ?? "unknown car-wasm worker error",
          {
            phase: error?.phase ?? this.#lastWorkerStage(pending),
            details: error?.details,
          },
        ),
      );
    };
    this.#worker.onerror = (event) => {
      this.#rejectPendingWith(
        "WorkerError",
        event?.message ?? event?.error?.message ?? "car-wasm worker failed",
        {
          phase: "worker-error",
          cause: event?.error,
        },
      );
    };
    this.#worker.onmessageerror = () => {
      this.#rejectPendingWith(
        "MessageError",
        "car-wasm worker message deserialization failed",
        {
          phase: "message-error",
        },
      );
    };
  }

  setDiagnosticListener(listener) {
    this.#diagnosticListener = typeof listener === "function" ? listener : null;
  }

  async documentInfo() {
    return this.#request("document_info");
  }

  async listEntries() {
    return this.#request("list_entries");
  }

  async listImages() {
    return this.#request("list_images");
  }

  async listEntrySummaries() {
    return this.#request("list_entry_summaries");
  }

  async listImageSummaries() {
    return this.#request("list_image_summaries");
  }

  async getEntryInfo(id) {
    return this.#request("get_entry_info", { id });
  }

  async getImageInfo(id) {
    return this.#request("get_image_info", { id });
  }

  async getDisplayPayload(id) {
    return this.#request("get_display_payload", { id });
  }

  async getDownloadPayload(id) {
    return this.#request("get_download_payload", { id });
  }

  async getThumbnailPayload(id, options = {}) {
    return this.#request("get_thumbnail_payload", { id, options });
  }

  terminate() {
    this.#worker.terminate();
    this.#rejectPendingWith(
      "Terminated",
      "car-wasm worker terminated before request completed",
      {
        phase: "terminated",
      },
    );
  }

  #request(type, payload = {}, transfer = []) {
    const requestId = this.#nextRequestId++;
    const promise = new Promise((resolve, reject) => {
      this.#pending.set(requestId, {
        requestId,
        type,
        resolve,
        reject,
        trace: [],
        timerId: null,
      });
    });
    const pending = this.#pending.get(requestId);
    const timeoutMs = this.#timeoutMs ?? defaultTimeoutMsFor(type);
    this.#recordTrace(pending, "client", "request-posted");
    try {
      this.#worker.postMessage({ requestId, type, payload }, transfer);
      if (timeoutMs > 0) {
        pending.timerId = globalThis.setTimeout(() => {
          const activePending = this.#pending.get(requestId);
          if (!activePending) {
            return;
          }
          this.#recordTrace(activePending, "client", "timeout");
          this.#pending.delete(requestId);
          const lastPhase = this.#lastWorkerStage(activePending);
          const trace = formatTrace(activePending.trace);
          const phaseSuffix = lastPhase
            ? ` (last phase: ${lastPhase})`
            : " (no worker diagnostics received before timeout)";
          const traceSuffix = trace ? `; trace: ${trace}` : "";
          activePending.reject(
            this.#createError(
              activePending,
              "Timeout",
              `car-wasm worker request "${type}" timed out after ${timeoutMs}ms${phaseSuffix}${traceSuffix}`,
              { phase: lastPhase ?? "timeout" },
            ),
          );
        }, timeoutMs);
      }
    } catch (error) {
      this.#pending.delete(requestId);
      pending.reject(
        this.#createError(
          pending,
          "PostMessageFailed",
          error?.message ?? "car-wasm worker postMessage failed",
          {
            phase: "post-message-failed",
            cause: error,
          },
        ),
      );
    }
    return promise;
  }

  #handleDiagnostic(diagnostic) {
    const requestId = diagnostic?.requestId;
    const pending =
      requestId == null ? null : this.#pending.get(requestId) ?? null;
    const event = {
      requestId,
      requestType:
        diagnostic?.requestType ?? pending?.type ?? "unknown",
      source: diagnostic?.source ?? "worker",
      stage: diagnostic?.stage ?? "unknown",
      timestamp:
        Number.isFinite(diagnostic?.timestamp) && diagnostic.timestamp > 0
          ? diagnostic.timestamp
          : timestampNow(),
    };
    if (diagnostic?.detail != null) {
      event.detail = diagnostic.detail;
    }
    if (pending) {
      pending.trace.push(event);
    }
    this.#emitDiagnostic(event);
  }

  #rejectPendingWith(code, message, options = {}) {
    for (const pending of this.#pending.values()) {
      this.#recordTrace(pending, "client", options.phase ?? "request-rejected");
      this.#clearTimer(pending);
      pending.reject(this.#createError(pending, code, message, options));
    }
    this.#pending.clear();
  }

  #createError(pending, code, message, options = {}) {
    return new CarWasmClientError(code, message, {
      requestId: pending?.requestId,
      requestType: pending?.type,
      phase: options.phase ?? this.#lastWorkerStage(pending),
      trace: pending?.trace?.slice() ?? [],
      details: options.details,
      cause: options.cause,
    });
  }

  #recordTrace(pending, source, stage, detail) {
    if (!pending) {
      return;
    }
    const event = {
      requestId: pending.requestId,
      requestType: pending.type,
      source,
      stage,
      timestamp: timestampNow(),
    };
    if (detail != null) {
      event.detail = detail;
    }
    pending.trace.push(event);
    this.#logDiagnostic(event);
    this.#emitDiagnostic(event);
  }

  #emitDiagnostic(event) {
    if (!this.#diagnosticListener) {
      return;
    }
    try {
      this.#diagnosticListener(event);
    } catch {}
  }

  #logDiagnostic(event) {
    if (!this.#consoleDiagnostics || event?.source !== "client") {
      return;
    }

    const prefix = `[car-wasm/client] ${event.requestType}#${event.requestId} ${event.source}:${event.stage}`;
    const detail = formatDiagnosticDetail(event.detail);
    const isWarning =
      event.stage === "timeout" ||
      event.stage === "worker-error" ||
      event.stage === "message-error" ||
      event.stage === "post-message-failed" ||
      event.stage === "terminated";
    const logger = isWarning ? console.warn : console.debug;
    if (detail.length > 0) {
      logger(prefix, detail);
      return;
    }
    logger(prefix);
  }

  #clearTimer(pending) {
    if (pending?.timerId == null) {
      return;
    }
    globalThis.clearTimeout(pending.timerId);
    pending.timerId = null;
  }

  #lastWorkerStage(pending) {
    const trace = pending?.trace ?? [];
    for (let index = trace.length - 1; index >= 0; index -= 1) {
      if (trace[index].source === "worker") {
        return trace[index].stage;
      }
    }
    return null;
  }
}
