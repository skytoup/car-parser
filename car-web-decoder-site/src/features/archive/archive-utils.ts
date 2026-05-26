import { zipSync } from "fflate";

import type { DownloadPayload } from "@/lib/types";

export function toUint8Array(bytes: Uint8Array | number[]): Uint8Array {
  if (bytes instanceof Uint8Array) {
    return bytes;
  }
  return new Uint8Array(bytes);
}

export function toOwnedArrayBuffer(bytes: Uint8Array | number[]): ArrayBuffer {
  return Uint8Array.from(toUint8Array(bytes)).buffer;
}

export function toUint8ClampedArray(
  bytes: Uint8Array | number[] | Uint8ClampedArray,
): Uint8ClampedArray {
  if (bytes instanceof Uint8ClampedArray) {
    return bytes;
  }
  return new Uint8ClampedArray(Array.from(bytes));
}

export async function rgbaToBlob(
  rgba: Uint8ClampedArray,
  width: number,
  height: number,
): Promise<Blob> {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    throw new Error("Failed to get 2d context");
  }

  const imageData = new ImageData(rgba as any, width, height);
  ctx.putImageData(imageData, 0, 0);

  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob) {
        resolve(blob);
      } else {
        reject(new Error("Failed to convert canvas to blob"));
      }
    }, "image/png");
  });
}

export function errorToMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }
  if (typeof error === "string" && error.trim().length > 0) {
    return error;
  }
  return "发生未知错误。";
}

export function buildImageBlob(payload: DownloadPayload): Blob {
  return new Blob([toOwnedArrayBuffer(payload.bytes)], {
    type: payload.mime_type,
  });
}

export function triggerBrowserDownload(blob: Blob, fileName: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  anchor.style.display = "none";
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  window.setTimeout(() => {
    URL.revokeObjectURL(url);
  }, 0);
}

export function sanitizeFileName(name: string): string {
  const replaced = name.replace(/[\\/:*?"<>|]+/g, "_").trim();
  if (replaced.length === 0) {
    return "asset.bin";
  }
  return replaced;
}

export function dedupeFileName(
  fileName: string,
  existingNames: Set<string>,
): string {
  const cleanName = sanitizeFileName(fileName);
  if (!existingNames.has(cleanName)) {
    existingNames.add(cleanName);
    return cleanName;
  }

  const extIndex = cleanName.lastIndexOf(".");
  const hasExt = extIndex > 0 && extIndex < cleanName.length - 1;
  const stem = hasExt ? cleanName.slice(0, extIndex) : cleanName;
  const ext = hasExt ? cleanName.slice(extIndex) : "";

  let counter = 2;
  while (true) {
    const next = `${stem} (${counter})${ext}`;
    if (!existingNames.has(next)) {
      existingNames.add(next);
      return next;
    }
    counter += 1;
  }
}

export function deriveZipName(sourceFileName: string | null): string {
  if (!sourceFileName) {
    return "car-assets.zip";
  }
  const trimmed = sourceFileName.trim();
  if (trimmed.length === 0) {
    return "car-assets.zip";
  }
  const dotIndex = trimmed.lastIndexOf(".");
  const stem = dotIndex > 0 ? trimmed.slice(0, dotIndex) : trimmed;
  return `${sanitizeFileName(stem)}-assets.zip`;
}

export function buildZipBlob(entries: Record<string, Uint8Array>): Blob {
  const bytes = zipSync(entries, { level: 0 });
  return new Blob([toOwnedArrayBuffer(bytes)], { type: "application/zip" });
}
