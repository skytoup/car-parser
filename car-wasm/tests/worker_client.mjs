import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { Worker } from "node:worker_threads";

import { CarWasmArchiveClient } from "../js/client.js";

function createNodeWorkerAdapter(url) {
  const worker = new Worker(url, { type: "module" });
  const adapter = {
    onmessage: null,
    onerror: null,
    onmessageerror: null,
    postMessage(message) {
      worker.postMessage(message);
    },
    terminate() {
      return worker.terminate();
    },
  };

  worker.on("message", (data) => {
    adapter.onmessage?.({ data });
  });
  worker.on("error", (error) => {
    adapter.onerror?.({ error, message: error.message });
  });
  worker.on("messageerror", () => {
    adapter.onmessageerror?.();
  });

  return adapter;
}

const bytes = await readFile(new URL("../../car-tests/data/Assets.car", import.meta.url));
const worker = createNodeWorkerAdapter(new URL("../js/worker.js", import.meta.url));
const diagnostics = [];
const client = await CarWasmArchiveClient.load(new Uint8Array(bytes), {
  worker,
  consoleDiagnostics: false,
  onDiagnostic(event) {
    diagnostics.push(event);
  },
});

assert.ok(
  diagnostics.some(
    (event) => event.requestType === "load" && event.stage === "archive-load-complete",
  ),
  "load diagnostics should expose worker archive phases",
);

const documentInfoPromise = client.documentInfo();
assert.ok(
  documentInfoPromise instanceof Promise,
  "documentInfo should be Promise-based on the page side",
);
const documentInfo = await documentInfoPromise;
assert.ok(documentInfo.RenditionCount > 0, "worker-backed client should load archive metadata");

const entries = await client.listEntries();
assert.ok(entries.length > 0, "worker-backed client should list full resource entries");
assert.ok(
  entries.some((entry) => entry.entry_kind === "color"),
  "full entry listing should include color resources",
);

const images = await client.listImages();
assert.ok(images.length > 0, "worker-backed client should list image entries");
assert.notEqual(
  images[0].preview_source_id,
  undefined,
  "full image entries should expose preview_source_id",
);

const summaries = await client.listImageSummaries();
assert.equal(
  summaries.length,
  images.length,
  "summary listing should keep the same number of entries",
);
assert.notEqual(
  summaries[0].preview_source_id,
  undefined,
  "summary entries should expose preview_source_id",
);
assert.deepEqual(
  summaries.map((entry) => entry.id),
  images.map((entry) => entry.id),
  "summary listing should preserve entry ordering and ids",
);

const entrySummaries = await client.listEntrySummaries();
assert.equal(
  entrySummaries.length,
  entries.length,
  "entry summary listing should keep the same number of entries",
);

const firstEntry = await client.getEntryInfo(entries[0].id);
assert.equal(firstEntry.id, entries[0].id, "single entry lookup should match list metadata");

const first = await client.getImageInfo(images[0].id);
assert.equal(first.id, images[0].id, "single-entry lookup should match list metadata");

const thumbnail = await client.getThumbnailPayload(first.id);
assert.ok(
  thumbnail.preview_strategy === "img-binary" ||
    thumbnail.preview_strategy === "download-only",
  "thumbnail payload should expose a supported preview strategy",
);

const display = await client.getDisplayPayload(first.id);
assert.ok(display.preview_strategy, "display payload should include preview strategy");

const download = await client.getDownloadPayload(first.id);
assert.ok(download.bytes.length > 0, "download payload should include bytes");

client.terminate();

class HangingWorker {
  constructor() {
    this.onmessage = null;
    this.terminateCalls = 0;
  }

  postMessage(message) {
    queueMicrotask(() => {
      this.onmessage?.({
        data: {
          kind: "diagnostic",
          diagnostic: {
            requestId: message.requestId,
            requestType: message.type,
            source: "worker",
            stage: "archive-load-start",
            timestamp: Date.now(),
            detail: "fixture load entered parser",
          },
        },
      });
    });
  }

  terminate() {
    this.terminateCalls += 1;
  }
}

const timeoutDiagnostics = [];
await assert.rejects(
  CarWasmArchiveClient.load(new Uint8Array([0]), {
    worker: new HangingWorker(),
    timeoutMs: 5,
    consoleDiagnostics: false,
    onDiagnostic(event) {
      timeoutDiagnostics.push(event);
    },
  }),
  (error) => {
    assert.equal(error.code, "Timeout");
    assert.equal(error.requestType, "load");
    assert.equal(error.phase, "archive-load-start");
    assert.ok(Array.isArray(error.trace), "timeout should retain per-request trace");
    assert.match(error.message, /archive-load-start/);
    assert.match(error.message, /worker:archive-load-start/);
    return true;
  },
  "load timeout should surface the last worker phase and trace summary",
);
assert.ok(
  timeoutDiagnostics.some((event) => event.stage === "archive-load-start"),
  "timeout path should still report worker diagnostics before failing",
);

const originalWorker = globalThis.Worker;
const createdWorkers = [];

class FailingWorker {
  constructor() {
    this.onmessage = null;
    this.terminateCalls = 0;
    createdWorkers.push(this);
  }

  postMessage(message) {
    queueMicrotask(() => {
      this.onmessage?.({
        data: {
          kind: "diagnostic",
          diagnostic: {
            requestId: message.requestId,
            requestType: message.type,
            source: "worker",
            stage: "archive-load-start",
            timestamp: Date.now(),
          },
        },
      });
      this.onmessage?.({
        data: {
          requestId: message.requestId,
          ok: false,
          error: {
            code: "ArchiveLoad",
            message: "broken archive",
          },
        },
      });
    });
  }

  terminate() {
    this.terminateCalls += 1;
  }
}

globalThis.Worker = FailingWorker;

try {
  await assert.rejects(
    CarWasmArchiveClient.load(new Uint8Array([0]), { consoleDiagnostics: false }),
    (error) => {
      assert.equal(error.code, "ArchiveLoad");
      assert.match(error.message, /broken archive/);
      assert.equal(error.phase, "archive-load-start");
      return true;
    },
    "default-created worker load failure should surface the worker error",
  );
  assert.equal(createdWorkers.length, 1, "load should create one default worker");
  assert.equal(
    createdWorkers[0].terminateCalls,
    1,
    "load failure should terminate the default-created worker",
  );
} finally {
  if (originalWorker === undefined) {
    delete globalThis.Worker;
  } else {
    globalThis.Worker = originalWorker;
  }
}

const originalWorkerForError = globalThis.Worker;
const bootstrapWorkers = [];

class BootstrapFailingWorker {
  constructor() {
    this.onmessage = null;
    this.onerror = null;
    this.terminateCalls = 0;
    bootstrapWorkers.push(this);
  }

  postMessage() {
    queueMicrotask(() => {
      this.onerror?.({
        message: "worker bootstrap failed",
        error: new Error("worker bootstrap failed"),
      });
    });
  }

  terminate() {
    this.terminateCalls += 1;
  }
}

globalThis.Worker = BootstrapFailingWorker;

try {
  await assert.rejects(
    CarWasmArchiveClient.load(new Uint8Array([0]), { consoleDiagnostics: false }),
    (error) => {
      assert.equal(error.code, "WorkerError");
      assert.match(error.message, /worker bootstrap failed/);
      return true;
    },
    "default-created worker bootstrap failure should reject pending load",
  );
  assert.equal(
    bootstrapWorkers.length,
    1,
    "load should create one default worker for bootstrap failure handling",
  );
  assert.equal(
    bootstrapWorkers[0].terminateCalls,
    1,
    "bootstrap failure should terminate the default-created worker",
  );
} finally {
  if (originalWorkerForError === undefined) {
    delete globalThis.Worker;
  } else {
    globalThis.Worker = originalWorkerForError;
  }
}
