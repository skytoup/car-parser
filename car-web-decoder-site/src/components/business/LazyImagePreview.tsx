import { useEffect, useState, memo } from "react";
import { useInView } from "react-intersection-observer";
import { Loader2, ImageOff, FileX, Download } from "lucide-react";

import { loadThumbnailView } from "@/features/archive/thumbnail-cache";
import type { CarArchiveClient, ImageListItem } from "@/lib/types";

interface LazyImagePreviewProps {
  client: CarArchiveClient;
  image: ImageListItem;
  loadToken: number;
  maxDimension?: number;
}

type LocalPreviewView =
  | { kind: "none"; reason: "empty" | "download-only" | "document" | "error"; message: string }
  | { kind: "img-binary"; url: string; mimeType: string }
  | { kind: "loading" };

export const LazyImagePreview = memo(function LazyImagePreview({
  client,
  image,
  loadToken,
  maxDimension = 256,
}: LazyImagePreviewProps) {
  const { ref, inView } = useInView({
    triggerOnce: true,
    rootMargin: "200px 0px",
  });

  const [view, setView] = useState<LocalPreviewView>({ kind: "loading" });

  useEffect(() => {
    setView({ kind: "loading" });
  }, [image.id]);

  useEffect(() => {
    if (!inView) {
      return;
    }

    let isMounted = true;

    loadThumbnailView({
      client,
      image,
      loadToken,
      maxDimension,
    })
      .then((nextView) => {
        if (!isMounted) return;
        setView(nextView);
      })
      .catch(() => {
        if (!isMounted) return;
        setView({ kind: "none", reason: "error", message: "加载失败" });
      });

    return () => {
      isMounted = false;
    };
  }, [client, image, inView, loadToken, maxDimension]);

  return (
    <div
      ref={ref}
      className="flex h-full w-full items-center justify-center overflow-hidden"
    >
      {view.kind === "loading" && (
        <Loader2 className="h-6 w-6 animate-spin text-primary/30" />
      )}

      {view.kind === "img-binary" && (
        <img
          src={view.url}
          alt={image.id}
          className="h-full w-full object-contain p-2"
          loading="lazy"
        />
      )}

      {view.kind === "none" && view.reason === "download-only" && (
        <div className="flex flex-col items-center text-muted-foreground/60">
          <Download className="h-6 w-6 mb-1 opacity-50" />
          <span className="text-[10px]">不支持预览</span>
        </div>
      )}

      {view.kind === "none" && view.reason === "document" && (
        <div className="flex flex-col items-center text-muted-foreground/60">
          <FileX className="h-6 w-6 mb-1 opacity-50" />
          <span className="text-[10px]">文档类型</span>
        </div>
      )}

      {view.kind === "none" && (view.reason === "error" || view.reason === "empty") && (
        <div className="flex flex-col items-center text-destructive/50">
          <ImageOff className="h-6 w-6 mb-1 opacity-50" />
          <span className="text-[10px]">{view.message}</span>
        </div>
      )}
    </div>
  );
});
