import type {
  CarArchiveClient,
  DocumentInfo,
  ImageInfo,
  ImageListItem,
  ThumbnailPayload,
} from "@/lib/types";
import { archiveReducer, createInitialArchiveState } from "@/features/archive/reducer";

function createMockClient(): CarArchiveClient {
  return {
    documentInfo: async () => ({}),
    listEntries: async () => [],
    listImages: async () => [],
    listEntrySummaries: async () => [],
    listImageSummaries: async () => [],
    getEntryInfo: async () => {
      throw new Error("not implemented");
    },
    getImageInfo: async () => {
      throw new Error("not implemented");
    },
    getDisplayPayload: async () => {
      throw new Error("not implemented");
    },
    getDownloadPayload: async () => {
      throw new Error("not implemented");
    },
    getThumbnailPayload: async (): Promise<ThumbnailPayload> => {
      throw new Error("not implemented");
    },
    terminate: () => {},
  };
}

function createImage(id: string): ImageInfo {
  return {
    id,
    preview_source_id: id,
    facet_name: "facet",
    rendition_name: "rendition",
    width: 100,
    height: 40,
    scale: 2,
    logical_layout: "fixed",
    resolved_encoding: "ARGB",
    entry_kind: "image",
    preview_strategy: "img-binary",
    download_strategy: "original",
    suggested_extension: "png",
    suggested_file_name: `${id}.png`,
    mime_type: "image/png",
    preserves_original_format: true,
    selection_reason: "original-browser-binary",
    downloadable: true,
    color_space: null,
    color_components: null,
    css_color: null,
  };
}

function createImageSummary(id: string): ImageListItem {
  const image = createImage(id);
  return {
    id: image.id,
    preview_source_id: image.preview_source_id,
    facet_name: image.facet_name,
    rendition_name: image.rendition_name,
    width: image.width,
    height: image.height,
    scale: image.scale,
    resolved_encoding: image.resolved_encoding,
    entry_kind: image.entry_kind,
    preview_strategy: image.preview_strategy,
    downloadable: image.downloadable,
    css_color: image.css_color,
  };
}

describe("archiveReducer", () => {
  it("在 load/success 后进入 ready，并在切换文件时重置选择与下载状态", () => {
    const tokenA = 1;
    const tokenB = 2;
    const client = createMockClient();
    const documentInfo: DocumentInfo = { Author: "test", Version: 1 };
    const image = createImageSummary("entry-1");

    let state = createInitialArchiveState();
    state = archiveReducer(state, {
      type: "load/start",
      token: tokenA,
      fileName: "first.car",
    });
    state = archiveReducer(state, {
      type: "load/success",
      token: tokenA,
      client,
      documentInfo,
      images: [image],
    });
    state = archiveReducer(state, { type: "entry/select", entryId: image.id });
    state = archiveReducer(state, {
      type: "download/batch-start",
      token: tokenA,
      total: 1,
    });

    expect(state.phase).toBe("ready");
    expect(state.selectedEntryId).toBe(image.id);
    expect(state.batchDownload.status).toBe("running");

    state = archiveReducer(state, {
      type: "load/start",
      token: tokenB,
      fileName: "second.car",
    });

    expect(state.phase).toBe("reading-file");
    expect(state.fileName).toBe("second.car");
    expect(state.loadToken).toBe(tokenB);
    expect(state.client).toBeNull();
    expect(state.images).toEqual([]);
    expect(state.selectedEntryId).toBeNull();
    expect(state.preview.status).toBe("idle");
    expect(state.batchDownload.status).toBe("idle");
    expect(state.batchDownload.total).toBe(0);
    expect(state.batchDownload.failures).toEqual([]);
  });

  it("处理 load/error 并清空文档和资源数据", () => {
    const token = 7;
    const client = createMockClient();
    const image = createImageSummary("entry-err");

    let state = createInitialArchiveState();
    state = archiveReducer(state, {
      type: "load/start",
      token,
      fileName: "broken.car",
    });
    state = archiveReducer(state, {
      type: "load/success",
      token,
      client,
      documentInfo: { Key: "Value" },
      images: [image],
    });
    state = archiveReducer(state, {
      type: "load/error",
      token,
      error: "解析失败",
    });

    expect(state.phase).toBe("error");
    expect(state.loadError).toBe("解析失败");
    expect(state.client).toBeNull();
    expect(state.documentInfo).toBeNull();
    expect(state.images).toEqual([]);
    expect(state.selectedEntryId).toBeNull();
    expect(state.preview.status).toBe("idle");
  });

  it("跟踪批量下载状态迁移：running -> success/partial-failure/error", () => {
    const token = 11;
    let state = createInitialArchiveState();

    state = archiveReducer(state, {
      type: "load/start",
      token,
      fileName: "ok.car",
    });
    state = archiveReducer(state, {
      type: "download/batch-start",
      token,
      total: 3,
    });
    state = archiveReducer(state, {
      type: "download/batch-progress",
      token,
      completed: 2,
    });

    expect(state.batchDownload.status).toBe("running");
    expect(state.batchDownload.completed).toBe(2);
    expect(state.batchDownload.total).toBe(3);

    state = archiveReducer(state, {
      type: "download/batch-finish",
      token,
      failures: [],
      archiveName: "pack.zip",
    });
    expect(state.batchDownload.status).toBe("success");
    expect(state.batchDownload.archiveName).toBe("pack.zip");
    expect(state.batchDownload.completed).toBe(3);

    state = archiveReducer(state, {
      type: "download/batch-start",
      token,
      total: 2,
    });
    state = archiveReducer(state, {
      type: "download/batch-finish",
      token,
      failures: [{ id: "a", fileName: "a.png", reason: "no data" }],
      archiveName: "partial.zip",
    });
    expect(state.batchDownload.status).toBe("partial-failure");
    expect(state.batchDownload.failures).toHaveLength(1);

    state = archiveReducer(state, {
      type: "download/batch-start",
      token,
      total: 1,
    });
    state = archiveReducer(state, {
      type: "download/batch-error",
      token,
      error: "zip 构建失败",
      failures: [],
    });
    expect(state.batchDownload.status).toBe("error");
    expect(state.batchDownload.error).toBe("zip 构建失败");
  });

  it("清空当前选择时保留批量下载失败汇总", () => {
    const token = 19;
    const image = createImageSummary("entry-keep-summary");

    let state = createInitialArchiveState();
    state = archiveReducer(state, {
      type: "load/start",
      token,
      fileName: "summary.car",
    });
    state = archiveReducer(state, {
      type: "load/success",
      token,
      client: createMockClient(),
      documentInfo: {},
      images: [image],
    });
    state = archiveReducer(state, { type: "entry/select", entryId: image.id });
    state = archiveReducer(state, {
      type: "download/batch-start",
      token,
      total: 2,
    });
    state = archiveReducer(state, {
      type: "download/batch-finish",
      token,
      failures: [{ id: "broken", fileName: "broken.png", reason: "missing bytes" }],
      archiveName: "summary.zip",
    });

    expect(state.selectedEntryId).toBe(image.id);
    expect(state.batchDownload.status).toBe("partial-failure");
    expect(state.batchDownload.failures).toHaveLength(1);

    state = archiveReducer(state, { type: "entry/clear-selection" });

    expect(state.selectedEntryId).toBeNull();
    expect(state.preview.status).toBe("idle");
    expect(state.batchDownload.status).toBe("partial-failure");
    expect(state.batchDownload.archiveName).toBe("summary.zip");
    expect(state.batchDownload.failures).toEqual([
      { id: "broken", fileName: "broken.png", reason: "missing bytes" },
    ]);
  });
});
