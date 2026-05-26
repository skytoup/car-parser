import { toOwnedArrayBuffer } from "@/features/archive/archive-utils";
import type { CarArchiveClient, ImageListItem, ThumbnailPayload } from "@/lib/types";

type CachedThumbnailView =
  | { kind: "img-binary"; url: string; mimeType: string }
  | { kind: "none"; reason: "download-only"; message: string };

type ThumbnailCacheRecord =
  | { status: "pending"; promise: Promise<CachedThumbnailView> }
  | { status: "ready"; view: CachedThumbnailView };

const thumbnailCache = new Map<string, ThumbnailCacheRecord>();
const sessionKeys = new Map<number, Set<string>>();

function cacheKey(loadToken: number, id: string, maxDimension: number): string {
  return `${loadToken}:${id}:${maxDimension}`;
}

function rememberSessionKey(loadToken: number, key: string) {
  const keys = sessionKeys.get(loadToken) ?? new Set<string>();
  keys.add(key);
  sessionKeys.set(loadToken, keys);
}

function buildView(payload: ThumbnailPayload): CachedThumbnailView {
  if (payload.preview_strategy === "img-binary") {
    const blob = new Blob([toOwnedArrayBuffer(payload.bytes)], {
      type: payload.mime_type,
    });
    return {
      kind: "img-binary",
      url: URL.createObjectURL(blob),
      mimeType: payload.mime_type,
    };
  }

  return {
    kind: "none",
    reason: "download-only",
    message: "不支持内联预览",
  };
}

export async function loadThumbnailView(options: {
  client: CarArchiveClient;
  image: ImageListItem;
  loadToken: number;
  maxDimension: number;
}): Promise<CachedThumbnailView> {
  const { client, image, loadToken, maxDimension } = options;
  const previewId = image.preview_source_id ?? image.id;
  const key = cacheKey(loadToken, previewId, maxDimension);
  rememberSessionKey(loadToken, key);

  const cached = thumbnailCache.get(key);
  if (cached?.status === "ready") {
    return cached.view;
  }
  if (cached?.status === "pending") {
    return cached.promise;
  }

  const promise = client
    .getThumbnailPayload(previewId, { maxDimension })
    .then((payload) => {
      const view = buildView(payload);
      thumbnailCache.set(key, { status: "ready", view });
      return view;
    })
    .catch((error) => {
      thumbnailCache.delete(key);
      throw error;
    });

  thumbnailCache.set(key, { status: "pending", promise });
  return promise;
}

export function disposeThumbnailSession(loadToken: number) {
  const keys = sessionKeys.get(loadToken);
  if (!keys) {
    return;
  }

  for (const key of keys) {
    const cached = thumbnailCache.get(key);
    if (cached?.status === "ready" && cached.view.kind === "img-binary") {
      URL.revokeObjectURL(cached.view.url);
    }
    thumbnailCache.delete(key);
  }

  sessionKeys.delete(loadToken);
}
