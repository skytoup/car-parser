import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  CarArchiveClient,
  DisplayPayload,
  DownloadPayload,
  ImageInfo,
  ImageListItem,
  ThumbnailPayload,
} from "@/lib/types";
import App from "@/App";
import { renderUI, setupUser } from "@/test/helpers";

const { loadMock } = vi.hoisted(() => ({
  loadMock: vi.fn(),
}));

vi.mock("@car-wasm/client", () => ({
  CarWasmArchiveClient: {
    load: loadMock,
  },
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function createDownloadOnlyImage(id: string): ImageInfo {
  return {
    id,
    preview_source_id: null,
    facet_name: "FacetA",
    rendition_name: "Banner",
    width: 320,
    height: 120,
    scale: 2,
    logical_layout: "fixed",
    resolved_encoding: "raw-data",
    entry_kind: "raw-data",
    preview_strategy: "download-only",
    download_strategy: "original",
    suggested_extension: "bin",
    suggested_file_name: "banner.bin",
    mime_type: "application/octet-stream",
    preserves_original_format: true,
    selection_reason: "download-only-original",
    downloadable: true,
    color_space: null,
    color_components: null,
    css_color: null,
  };
}

function createColorEntry(id: string): ImageInfo {
  return {
    id,
    preview_source_id: id,
    facet_name: "Color/system",
    rendition_name: "System Purple",
    width: 0,
    height: 0,
    scale: 1,
    logical_layout: "color",
    resolved_encoding: "none",
    entry_kind: "color",
    preview_strategy: "color-swatch",
    download_strategy: "none",
    suggested_extension: "",
    suggested_file_name: "",
    mime_type: "",
    preserves_original_format: true,
    selection_reason: "metadata-color",
    downloadable: false,
    color_space: "system-srgb",
    color_components: [0.38, 0.333, 0.961, 1],
    css_color: "rgba(97, 85, 245, 1.0000)",
  };
}

function createDownloadOnlyDisplayPayload(image: ImageInfo): DisplayPayload {
  return {
    preview_strategy: "download-only",
    mime_type: image.mime_type,
    suggested_extension: image.suggested_extension,
    suggested_file_name: image.suggested_file_name,
    preserves_original_format: true,
    selection_reason: "download-only-original",
  };
}

function createColorDisplayPayload(image: ImageInfo): DisplayPayload {
  return {
    preview_strategy: "color-swatch",
    color_space: image.color_space ?? "srgb",
    components: image.color_components ?? [],
    css_color: image.css_color ?? "rgba(0, 0, 0, 1.0000)",
  };
}

function createBinaryDisplayPayload(): DisplayPayload {
  return {
    preview_strategy: "img-binary",
    mime_type: "image/png",
    suggested_extension: "png",
    suggested_file_name: "derived-preview.png",
    preserves_original_format: false,
    selection_reason: "decoded-raster",
    bytes: [1, 2, 3],
  };
}

function createThumbnailPayload(id: string, image: ImageInfo): ThumbnailPayload {
  if (image.entry_kind === "color") {
    return { preview_strategy: "download-only" };
  }

  const previewSourceId = image.preview_source_id;
  if (
    previewSourceId !== null &&
    previewSourceId !== image.id &&
    id === previewSourceId
  ) {
    return {
      preview_strategy: "img-binary",
      mime_type: "image/png",
      bytes: [1, 2, 3],
    };
  }

  return image.preview_strategy === "download-only"
    ? { preview_strategy: "download-only" }
    : {
        preview_strategy: "img-binary",
        mime_type: "image/png",
        bytes: [1, 2, 3],
      };
}

function toSummary(image: ImageInfo): ImageListItem {
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

function createDownloadPayload(image: ImageInfo): DownloadPayload {
  return {
    bytes: [11, 12, 13],
    mime_type: image.mime_type,
    suggested_extension: image.suggested_extension,
    suggested_file_name: image.suggested_file_name,
    download_strategy: "original",
    preserves_original_format: true,
    selection_reason: "download-only-original",
  };
}

function createClient(image: ImageInfo): CarArchiveClient {
  return {
    documentInfo: vi.fn().mockResolvedValue({ Platform: "iOS", Version: 17 }),
    listEntries: vi.fn().mockResolvedValue([image]),
    listImages: vi.fn().mockResolvedValue([image]),
    listEntrySummaries: vi.fn().mockResolvedValue([toSummary(image)]),
    listImageSummaries: vi.fn().mockResolvedValue([toSummary(image)]),
    getEntryInfo: vi.fn().mockResolvedValue(image),
    getImageInfo: vi.fn().mockResolvedValue(image),
    getDisplayPayload: vi.fn().mockImplementation(async (id: string) => {
      if (image.entry_kind === "color") {
        return createColorDisplayPayload(image);
      }
      if (id === image.preview_source_id && image.preview_source_id !== image.id) {
        return createBinaryDisplayPayload();
      }
      return createDownloadOnlyDisplayPayload(image);
    }),
    getDownloadPayload: vi.fn().mockResolvedValue(createDownloadPayload(image)),
    getThumbnailPayload: vi
      .fn()
      .mockImplementation(async (id: string) => createThumbnailPayload(id, image)),
    terminate: vi.fn(),
  };
}

function createPartialFailureClient(
  images: ImageInfo[],
  failingId: string,
): CarArchiveClient {
  return {
    documentInfo: vi.fn().mockResolvedValue({ Platform: "iOS", Version: 17 }),
    listEntries: vi.fn().mockResolvedValue(images),
    listImages: vi.fn().mockResolvedValue(images),
    listEntrySummaries: vi.fn().mockResolvedValue(images.map(toSummary)),
    listImageSummaries: vi.fn().mockResolvedValue(images.map(toSummary)),
    getEntryInfo: vi.fn().mockImplementation(async (id: string) => {
      const image = images.find((item) => item.id === id);
      if (!image) {
        throw new Error(`missing image: ${id}`);
      }
      return image;
    }),
    getImageInfo: vi.fn().mockImplementation(async (id: string) => {
      const image = images.find((item) => item.id === id);
      if (!image) {
        throw new Error(`missing image: ${id}`);
      }
      return image;
    }),
    getDisplayPayload: vi.fn().mockImplementation(async (id: string) => {
      const image = images.find((item) => item.id === id);
      if (!image) {
        const previewOwner = images.find((item) => item.preview_source_id === id);
        if (!previewOwner) {
          throw new Error(`missing image: ${id}`);
        }
        return createBinaryDisplayPayload();
      }
      if (image.entry_kind === "color") {
        return createColorDisplayPayload(image);
      }
      return createDownloadOnlyDisplayPayload(image);
    }),
    getDownloadPayload: vi.fn().mockImplementation(async (id: string) => {
      const image = images.find((item) => item.id === id);
      if (!image) {
        throw new Error(`missing image: ${id}`);
      }
      if (id === failingId) {
        throw new Error("条目损坏");
      }
      return createDownloadPayload(image);
    }),
    getThumbnailPayload: vi.fn().mockImplementation(async (id: string) => {
      const image =
        images.find((item) => item.id === id) ??
        images.find((item) => item.preview_source_id === id);
      if (!image) {
        throw new Error(`missing image: ${id}`);
      }
      return createThumbnailPayload(id, image);
    }),
    terminate: vi.fn(),
  };
}

describe("App 交互", () => {
  beforeAll(() => {
    if (typeof URL.createObjectURL !== "function") {
      Object.defineProperty(URL, "createObjectURL", {
        configurable: true,
        writable: true,
        value: () => "blob:mock",
      });
    }
    if (typeof URL.revokeObjectURL !== "function") {
      Object.defineProperty(URL, "revokeObjectURL", {
        configurable: true,
        writable: true,
        value: () => {},
      });
    }
  });

  beforeEach(() => {
    loadMock.mockReset();
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:download");
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  });

  it("覆盖加载阶段、错误恢复、派生预览与原始下载流程", async () => {
    const user = setupUser();
    const image = {
      ...createDownloadOnlyImage("entry-download-only"),
      preview_source_id: "entry-download-only-preview",
      suggested_extension: "heic",
      suggested_file_name: "banner.heic",
      mime_type: "image/heic",
    };
    const client = createClient(image);
    const secondLoad = deferred<CarArchiveClient>();

    loadMock
      .mockRejectedValueOnce(new Error("首轮加载失败"))
      .mockReturnValueOnce(secondLoad.promise);

    renderUI(<App />);
    const input = screen.getByLabelText("选择本地文件");

    await user.upload(
      input,
      new File([new Uint8Array([1, 2, 3])], "broken.car", {
        type: "application/octet-stream",
      }),
    );

    expect(await screen.findByText("首轮加载失败")).toBeInTheDocument();
    expect(screen.getByText("加载失败")).toBeInTheDocument();

    await user.upload(
      input,
      new File([new Uint8Array([4, 5, 6])], "fixed.car", {
        type: "application/octet-stream",
      }),
    );

    expect(await screen.findByText("正在初始化解析器")).toBeInTheDocument();

    secondLoad.resolve(client);

    const entryButton = await screen.findByRole("button", {
      name: /FacetA \/ Banner/,
    });
    await waitFor(() => {
      expect(client.listEntrySummaries).toHaveBeenCalledTimes(1);
    });
    expect(client.listImageSummaries).not.toHaveBeenCalled();
    expect(client.listImages).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(client.getThumbnailPayload).toHaveBeenCalledWith(
        image.preview_source_id,
        {
          maxDimension: 256,
        },
      );
    });
    expect(client.getThumbnailPayload).not.toHaveBeenCalledWith(image.id, {
        maxDimension: 256,
      });
    expect(client.getDisplayPayload).not.toHaveBeenCalled();
    expect(screen.queryByText("首轮加载失败")).not.toBeInTheDocument();

    await user.click(entryButton);
    await waitFor(() => {
      expect(client.getEntryInfo).toHaveBeenCalledWith(image.id);
      expect(client.getDisplayPayload).toHaveBeenCalledWith(
        image.preview_source_id,
      );
    });
    expect(
      await screen.findByText(
        /当前显示的是关联条目的派生预览（来源 ID: entry-download-only-preview）/,
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("该资源仅支持下载，不提供可视化预览。"),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "下载当前资源" }));

    await waitFor(() => {
      expect(client.getDownloadPayload).toHaveBeenCalledWith(image.id);
    });
    expect(URL.createObjectURL).toHaveBeenCalled();
    expect(HTMLAnchorElement.prototype.click).toHaveBeenCalled();
  });

  it("批量下载部分失败后，失败汇总不会因清空选择而消失", async () => {
    const user = setupUser();
    const firstImage = createDownloadOnlyImage("entry-ok");
    const secondImage = {
      ...createDownloadOnlyImage("entry-broken"),
      facet_name: "FacetB",
      rendition_name: "Broken",
      suggested_file_name: "broken.bin",
    };
    const client = createPartialFailureClient(
      [firstImage, secondImage],
      secondImage.id,
    );

    loadMock.mockResolvedValueOnce(client);

    renderUI(<App />);

    await user.upload(
      screen.getByLabelText("选择本地文件"),
      new File([new Uint8Array([7, 8, 9])], "partial.car", {
        type: "application/octet-stream",
      }),
    );

    const firstEntryButton = await screen.findByRole("button", {
      name: /FacetA \/ Banner/,
    });
    await user.click(firstEntryButton);

    await user.click(screen.getByRole("button", { name: "下载全部 (ZIP)" }));

    expect(
      await screen.findAllByText("已完成下载，部分失败 1 项。"),
    ).not.toHaveLength(0);
    expect(screen.getByText("批量下载失败汇总")).toBeInTheDocument();
    expect(screen.getByText("entry-broken.bin")).toBeInTheDocument();
    expect(screen.getByText("条目损坏")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "清空选择" }));

    expect(screen.queryByText("当前条目 ID")).not.toBeInTheDocument();
    expect(screen.getByText("批量下载失败汇总")).toBeInTheDocument();
    expect(screen.getByText("entry-broken.bin")).toBeInTheDocument();
    expect(screen.getByText("条目损坏")).toBeInTheDocument();
  });

  it("使用全量资源列表展示 Lottie JSON raw-data", async () => {
    const user = setupUser();
    const lottie = {
      ...createDownloadOnlyImage("entry-lottie-json"),
      facet_name: "Lottie/TimelineReply/close",
      rendition_name: "close.json",
      suggested_extension: "json",
      suggested_file_name: "close.json",
      mime_type: "application/json",
    };
    const client = createClient(lottie);
    vi.mocked(client.listImageSummaries).mockResolvedValue([]);

    loadMock.mockResolvedValueOnce(client);

    renderUI(<App />);

    await user.upload(
      screen.getByLabelText("选择本地文件"),
      new File([new Uint8Array([1, 1, 1])], "lottie.car", {
        type: "application/octet-stream",
      }),
    );

    expect(
      await screen.findByRole("button", {
        name: /Lottie\/TimelineReply\/close \/ close\.json/,
      }),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(client.listEntrySummaries).toHaveBeenCalledTimes(1);
    });
    expect(client.listImageSummaries).not.toHaveBeenCalled();
  });

  it("颜色资源会显示在列表中，并以色块形式展示且禁用下载", async () => {
    const user = setupUser();
    const color = createColorEntry("entry-color");
    const client = createClient(color);

    loadMock.mockResolvedValueOnce(client);

    renderUI(<App />);

    await user.upload(
      screen.getByLabelText("选择本地文件"),
      new File([new Uint8Array([5, 5, 5])], "colors.car", {
        type: "application/octet-stream",
      }),
    );

    const entryButton = await screen.findByRole("button", {
      name: /Color\/system \/ System Purple/,
    });
    expect(entryButton).toBeInTheDocument();

    await user.click(entryButton);

    expect(await screen.findByText("Color Space")).toBeInTheDocument();
    expect(screen.getAllByText("system-srgb").length).toBeGreaterThan(0);
    expect(screen.getByText("rgba(97, 85, 245, 1.0000)")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "下载当前资源" })).toBeDisabled();
    expect(client.getDownloadPayload).not.toHaveBeenCalled();
  });

  it("大列表只挂载可见窗口附近的条目，并在滚动后切换可见项", async () => {
    const user = setupUser();
    const images = Array.from({ length: 90 }, (_, index) => ({
      ...createDownloadOnlyImage(`entry-${index}`),
      facet_name: `Facet ${index}`,
      rendition_name: `Asset ${index}`,
      suggested_file_name: `asset-${index}.bin`,
    }));
    const client = createPartialFailureClient(images, "never-fails");

    loadMock.mockResolvedValueOnce(client);

    renderUI(<App />);

    await user.upload(
      screen.getByLabelText("选择本地文件"),
      new File([new Uint8Array([9, 8, 7])], "virtualized.car", {
        type: "application/octet-stream",
      }),
    );

    const viewport = await screen.findByTestId("resource-list-viewport");
    const thumbnailMock = vi.mocked(client.getThumbnailPayload);
    await waitFor(() => {
      expect(thumbnailMock.mock.calls.length).toBeLessThan(images.length);
    });
    expect(
      screen.queryByRole("button", { name: /Facet 40 \/ Asset 40/ }),
    ).not.toBeInTheDocument();

    Object.defineProperty(viewport, "scrollTop", {
      configurable: true,
      writable: true,
      value: 280 * 10,
    });
    fireEvent.scroll(viewport, { target: { scrollTop: 280 * 10 } });

    expect(
      await screen.findByRole("button", { name: /Facet 40 \/ Asset 40/ }),
    ).toBeInTheDocument();
  });
});
