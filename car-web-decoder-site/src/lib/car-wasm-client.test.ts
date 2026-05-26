import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  CarWasmArchiveClient,
  CarWasmClientError,
  type CarWasmClientDiagnosticEvent,
} from "@car-wasm/client";

class FakeWorker {
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  postMessage = vi.fn();
  terminate = vi.fn();
}

describe("CarWasmArchiveClient 诊断", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(console, "debug").mockImplementation(() => {});
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("在 load 超时时抛出带 trace 的错误", async () => {
    const rawWorker = new FakeWorker();
    const worker = rawWorker as unknown as Worker;

    rawWorker.postMessage.mockImplementation((message) => {
      rawWorker.onmessage?.({
        data: {
          kind: "diagnostic",
          diagnostic: {
            requestId: message.requestId,
            requestType: message.type,
            source: "worker",
            stage: "archive-load-start",
            timestamp: Date.now(),
            detail: "byteLength=3",
          },
        },
      });
      rawWorker.onmessage?.({
        data: {
          kind: "diagnostic",
          diagnostic: {
            requestId: message.requestId,
            requestType: message.type,
            source: "worker",
            stage: "wasm-module-load-complete",
            timestamp: Date.now(),
          },
        },
      });
    });

    const loadPromise = CarWasmArchiveClient.load(new Uint8Array([1, 2, 3]), {
      worker,
      timeoutMs: 25,
      consoleDiagnostics: false,
    });
    const rejection = loadPromise.catch((reason) => reason);

    await vi.advanceTimersByTimeAsync(25);

    const error = await rejection;
    expect(error).toBeInstanceOf(CarWasmClientError);
    expect(String(error?.message ?? "")).toMatch(/timed out/);
    expect(String(error?.message ?? "")).toMatch(/archive-load-start/);
    expect(String(error?.message ?? "")).toMatch(/wasm-module-load-complete/);
    expect(rawWorker.terminate).not.toHaveBeenCalled();
  });

  it("会把 worker 诊断事件透传给监听器", async () => {
    const rawWorker = new FakeWorker();
    const worker = rawWorker as unknown as Worker;
    const onDiagnostic = vi.fn<(event: CarWasmClientDiagnosticEvent) => void>();

    rawWorker.postMessage.mockImplementation((message) => {
      rawWorker.onmessage?.({
        data: {
          kind: "diagnostic",
          diagnostic: {
            requestId: message.requestId,
            requestType: message.type,
            source: "worker",
            stage: "document-info-start",
            timestamp: Date.now(),
          },
        },
      });
      rawWorker.onmessage?.({
        data: {
          kind: "response",
          requestId: message.requestId,
          ok: true,
          result: { Version: 1 },
        },
      });
    });

    const client = new CarWasmArchiveClient(worker, {
      onDiagnostic,
      consoleDiagnostics: false,
    });

    await expect(client.documentInfo()).resolves.toEqual({ Version: 1 });
    expect(onDiagnostic).toHaveBeenCalledWith(
      expect.objectContaining({
        source: "worker",
        stage: "document-info-start",
        requestType: "document_info",
      }),
    );
  });

  it("支持 entry 列表、摘要列表和缩略图请求", async () => {
    const rawWorker = new FakeWorker();
    const worker = rawWorker as unknown as Worker;

    rawWorker.postMessage.mockImplementation((message) => {
      if (message.type === "list_entries") {
        rawWorker.onmessage?.({
          data: {
            kind: "response",
            requestId: message.requestId,
            ok: true,
            result: [{ id: "entry-1", entry_kind: "raw-data" }],
          },
        });
        return;
      }

      if (message.type === "get_entry_info") {
        rawWorker.onmessage?.({
          data: {
            kind: "response",
            requestId: message.requestId,
            ok: true,
            result: { id: "entry-1", entry_kind: "raw-data" },
          },
        });
        return;
      }

      if (message.type === "list_image_summaries") {
        rawWorker.onmessage?.({
          data: {
            kind: "response",
            requestId: message.requestId,
            ok: true,
            result: [{ id: "entry-1" }],
          },
        });
        return;
      }

      if (message.type === "list_entry_summaries") {
        rawWorker.onmessage?.({
          data: {
            kind: "response",
            requestId: message.requestId,
            ok: true,
            result: [{ id: "entry-1", entry_kind: "raw-data" }],
          },
        });
        return;
      }

      if (message.type === "get_thumbnail_payload") {
        rawWorker.onmessage?.({
          data: {
            kind: "response",
            requestId: message.requestId,
            ok: true,
            result: { preview_strategy: "download-only" },
          },
        });
      }
    });

    const client = new CarWasmArchiveClient(worker, {
      consoleDiagnostics: false,
    });

    await expect(client.listEntries()).resolves.toEqual([
      { id: "entry-1", entry_kind: "raw-data" },
    ]);
    await expect(client.getEntryInfo("entry-1")).resolves.toEqual({
      id: "entry-1",
      entry_kind: "raw-data",
    });
    await expect(client.listEntrySummaries()).resolves.toEqual([
      { id: "entry-1", entry_kind: "raw-data" },
    ]);
    await expect(client.listImageSummaries()).resolves.toEqual([{ id: "entry-1" }]);
    await expect(
      client.getThumbnailPayload("entry-1", { maxDimension: 128 }),
    ).resolves.toEqual({
      preview_strategy: "download-only",
    });
    expect(rawWorker.postMessage).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({
        type: "list_entries",
      }),
      [],
    );
    expect(rawWorker.postMessage).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({
        type: "get_entry_info",
        payload: { id: "entry-1" },
      }),
      [],
    );
    expect(rawWorker.postMessage).toHaveBeenNthCalledWith(
      3,
      expect.objectContaining({
        type: "list_entry_summaries",
      }),
      [],
    );
    expect(rawWorker.postMessage).toHaveBeenNthCalledWith(
      4,
      expect.objectContaining({
        type: "list_image_summaries",
      }),
      [],
    );
    expect(rawWorker.postMessage).toHaveBeenNthCalledWith(
      5,
      expect.objectContaining({
        type: "get_thumbnail_payload",
        payload: {
          id: "entry-1",
          options: { maxDimension: 128 },
        },
      }),
      [],
    );
  });
});
