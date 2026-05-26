import {
  type ChangeEvent,
  type DragEvent,
  useEffect,
  useReducer,
  useRef,
  useState,
  useCallback,
  useMemo,
  startTransition,
  useDeferredValue,
} from "react";
import { useTranslation } from "react-i18next";
import { CarWasmArchiveClient } from "@car-wasm/client";
import {
  Search,
  ImageOff,
  FileX,
  Inbox,
  Loader2,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Languages,
  Download,
  Sun,
  Moon,
  Monitor,
  ExternalLink,
} from "lucide-react";
import { Toaster, toast } from "sonner";
import Zoom from "react-medium-image-zoom";

import {
  Badge,
  Button,
  buttonVariants,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Progress,
} from "@/components/ui";
import { cn } from "@/lib/cn";
import {
  buildImageBlob,
  buildZipBlob,
  dedupeFileName,
  deriveZipName,
  errorToMessage,
  sanitizeFileName,
  toUint8Array,
  toOwnedArrayBuffer,
  triggerBrowserDownload,
} from "@/features/archive/archive-utils";
import { CanvasPreview } from "@/features/archive/CanvasPreview";
import { disposeThumbnailSession } from "@/features/archive/thumbnail-cache";
import { ResourceCard } from "@/components/business/ResourceCard";

import {
  archiveReducer,
  createInitialArchiveState,
  type PreviewView,
} from "@/features/archive/reducer";
import type {
  BatchDownloadFailure,
  CarArchiveClient,
  DisplayPayload,
  DocumentInfo,
  DownloadPayload,
  ImageInfo,
  ImageListItem,
  LoadPhase,
} from "@/lib/types";

type Theme = "light" | "dark" | "auto";

function readStoredTheme(): Theme {
  if (typeof window === "undefined") {
    return "auto";
  }

  try {
    const value = window.localStorage?.getItem?.("theme");
    return value === "light" || value === "dark" || value === "auto"
      ? value
      : "auto";
  } catch {
    return "auto";
  }
}

function writeStoredTheme(theme: Theme) {
  try {
    window.localStorage?.setItem?.("theme", theme);
  } catch {
    // Ignore storage failures in restricted or mocked browser environments.
  }
}

const PHASE_STEP: Record<LoadPhase, number> = {
  idle: 0,
  "reading-file": 1,
  "loading-archive": 2,
  "listing-assets": 3,
  ready: 4,
  error: 4,
};

const LIST_ITEM_HEIGHT = 280;
const LIST_OVERSCAN_ROWS = 3;
const LIST_VIEWPORT_HEIGHT = 560;
const LIST_THUMBNAIL_DIMENSION = 256;

interface SearchableImageItem {
  image: ImageListItem;
  label: string;
  searchKeyLower: string;
}

interface ImageLabelLike {
  id: string;
  facet_name: string;
  rendition_name: string;
}

interface PreviewSourceLike {
  id: string;
  preview_source_id: string | null;
}

function normalizeDocumentInfo(raw: Record<string, unknown>): DocumentInfo {
  const normalized: DocumentInfo = {};
  for (const [key, value] of Object.entries(raw)) {
    if (
      typeof value === "string" ||
      typeof value === "number" ||
      typeof value === "boolean" ||
      value === null
    ) {
      normalized[key] = value;
      continue;
    }

    if (value === undefined) {
      normalized[key] = "undefined";
      continue;
    }

    try {
      normalized[key] = JSON.stringify(value);
    } catch {
      normalized[key] = String(value);
    }
  }
  return normalized;
}

function fallbackFileName(
  id: string,
  preferredName: string | null | undefined,
  preferredExtension: string | null | undefined,
): string {
  const trimmedName = preferredName?.trim() ?? "";
  if (trimmedName.length > 0) {
    return sanitizeFileName(trimmedName);
  }

  const trimmedExt = preferredExtension?.trim().replace(/^\./, "") ?? "";
  if (trimmedExt.length > 0) {
    return sanitizeFileName(`${id}.${trimmedExt}`);
  }

  return sanitizeFileName(`${id}.bin`);
}

function formatImageLabel(image: ImageLabelLike): string {
  const facet = image.facet_name.trim();
  const rendition = image.rendition_name.trim();
  if (facet.length > 0 && rendition.length > 0) {
    return `${facet} / ${rendition}`;
  }
  if (facet.length > 0) {
    return facet;
  }
  if (rendition.length > 0) {
    return rendition;
  }
  return image.id;
}

function formatDocumentValue(value: DocumentInfo[string]): string {
  if (typeof value === "boolean") {
    return value ? "true" : "false";
  }
  return String(value);
}

function resolvePreviewEntryId(entry: PreviewSourceLike): string {
  return entry.preview_source_id ?? entry.id;
}

async function readFileBytes(file: File): Promise<Uint8Array> {
  if (typeof file.arrayBuffer === "function") {
    return new Uint8Array(await file.arrayBuffer());
  }

  if (typeof Blob.prototype.arrayBuffer === "function") {
    return new Uint8Array(await Blob.prototype.arrayBuffer.call(file));
  }

  return new Uint8Array(await new Response(file).arrayBuffer());
}

function App() {
  const { t, i18n } = useTranslation();
  const [state, dispatch] = useReducer(
    archiveReducer,
    undefined,
    createInitialArchiveState,
  );
  const [isDragOver, setIsDragOver] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [isDocInfoExpanded, setIsDocInfoExpanded] = useState(false);
  const [isAdvancedParamsExpanded, setIsAdvancedParamsExpanded] =
    useState(false);
  const [isLgViewport, setIsLgViewport] = useState(
    () => typeof window !== "undefined" && window.innerWidth >= 1024,
  );
  const [listScrollTop, setListScrollTop] = useState(0);

  const [theme, setTheme] = useState<Theme>(() => {
    return readStoredTheme();
  });

  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const listViewportRef = useRef<HTMLDivElement | null>(null);
  const sessionTokenRef = useRef(0);
  const selectedEntryRef = useRef<string | null>(null);
  const activeClientRef = useRef<CarArchiveClient | null>(null);
  const previewUrlRef = useRef<string | null>(null);
  const stateRef = useRef(state);
  stateRef.current = state;

  useEffect(() => {
    const applyTheme = () => {
      const isDark =
        theme === "dark" ||
        (theme === "auto" &&
          window.matchMedia("(prefers-color-scheme: dark)").matches);
      document.documentElement.classList.toggle("dark", isDark);
    };

    applyTheme();
    writeStoredTheme(theme);

    if (theme === "auto") {
      const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
      const listener = () => applyTheme();
      mediaQuery.addEventListener("change", listener);
      return () => mediaQuery.removeEventListener("change", listener);
    }
  }, [theme]);

  selectedEntryRef.current = state.selectedEntryId;

  const revokePreviewUrl = () => {
    if (!previewUrlRef.current) {
      return;
    }
    URL.revokeObjectURL(previewUrlRef.current);
    previewUrlRef.current = null;
  };

  const terminateActiveClient = () => {
    if (!activeClientRef.current) {
      return;
    }
    try {
      activeClientRef.current.terminate();
    } catch {
      // Ignore terminate errors during session switch.
    }
    activeClientRef.current = null;
  };

  const isActiveSession = (token: number): boolean =>
    sessionTokenRef.current === token;
  const isActiveSelection = (token: number, entryId: string): boolean =>
    isActiveSession(token) && selectedEntryRef.current === entryId;

  const buildPreviewView = async (
    payload: DisplayPayload,
    t: any,
  ): Promise<PreviewView> => {
    switch (payload.preview_strategy) {
      case "img-binary": {
        revokePreviewUrl();
        const blob = new Blob([toOwnedArrayBuffer(payload.bytes)], {
          type: payload.mime_type,
        });
        const url = URL.createObjectURL(blob);
        previewUrlRef.current = url;
        return {
          kind: "img-binary",
          url,
          mimeType: payload.mime_type,
        };
      }
      case "canvas-rgba": {
        revokePreviewUrl();
        return {
          kind: "canvas-rgba",
          width: payload.width,
          height: payload.height,
          rgba: new Uint8ClampedArray(toUint8Array(payload.rgba)),
        };
      }
      case "document": {
        revokePreviewUrl();
        return {
          kind: "none",
          reason: "document",
          message: t("preview.document"),
        };
      }
      case "download-only": {
        revokePreviewUrl();
        return {
          kind: "none",
          reason: "download-only",
          message: t("preview.downloadOnly"),
        };
      }
      case "color-swatch": {
        revokePreviewUrl();
        return {
          kind: "color-swatch",
          colorSpace: payload.color_space,
          components: payload.components,
          cssColor: payload.css_color,
        };
      }
      default:
        return {
          kind: "none",
          reason: "empty",
          message: t("preview.noSelection"),
        };
    }
  };

  const startLoadSession = (fileName: string): number => {
    disposeThumbnailSession(sessionTokenRef.current);
    const token = sessionTokenRef.current + 1;
    sessionTokenRef.current = token;

    revokePreviewUrl();
    terminateActiveClient();
    selectedEntryRef.current = null;
    setListScrollTop(0);

    if (listViewportRef.current) {
      listViewportRef.current.scrollTop = 0;
    }

    dispatch({ type: "load/start", token, fileName });
    return token;
  };

  const loadArchiveFile = async (file: File) => {
    const token = startLoadSession(file.name);

    try {
      const bytes = await readFileBytes(file);
      if (!isActiveSession(token)) {
        return;
      }

      dispatch({ type: "load/phase", token, phase: "loading-archive" });

      const loadedClient = (await CarWasmArchiveClient.load(
        bytes,
      )) as unknown as CarArchiveClient;
      if (!isActiveSession(token)) {
        loadedClient.terminate();
        return;
      }

      const documentInfoRaw = (await loadedClient.documentInfo()) as Record<
        string,
        unknown
      >;
      if (!isActiveSession(token)) {
        loadedClient.terminate();
        return;
      }

      dispatch({ type: "load/phase", token, phase: "listing-assets" });
      const images =
        (await loadedClient.listEntrySummaries()) as ImageListItem[];
      if (!isActiveSession(token)) {
        loadedClient.terminate();
        return;
      }

      activeClientRef.current = loadedClient;
      startTransition(() => {
        dispatch({
          type: "load/success",
          token,
          client: loadedClient,
          documentInfo: normalizeDocumentInfo(documentInfoRaw),
          images,
        });
      });
    } catch (error) {
      if (!isActiveSession(token)) {
        return;
      }
      dispatch({
        type: "load/error",
        token,
        error: errorToMessage(error),
      });
    }
  };

  const selectEntry = useCallback((entryId: string, options?: { force?: boolean }) => {
    const currentState = stateRef.current;
    if (!options?.force && entryId === selectedEntryRef.current) {
      return;
    }

    if (currentState.phase !== "ready" || !currentState.client) {
      return;
    }

    const token = currentState.loadToken;
    const client = currentState.client;
    const selectedSummary = currentState.images.find((image) => image.id === entryId);
    const previewEntryId = selectedSummary
      ? resolvePreviewEntryId(selectedSummary)
      : entryId;
    selectedEntryRef.current = entryId;

    dispatch({ type: "entry/select", entryId });
    dispatch({ type: "preview/loading", token, entryId });

    void (async () => {
      try {
        const info = (await client.getEntryInfo(entryId)) as ImageInfo;
        if (!isActiveSelection(token, entryId)) {
          return;
        }
        dispatch({ type: "entry/info-ready", token, entryId, info });
      } catch (error) {
        if (!isActiveSelection(token, entryId)) {
          return;
        }
        dispatch({
          type: "entry/info-error",
          token,
          entryId,
          error: errorToMessage(error),
        });
      }
    })();

    void (async () => {
      try {
        const payload = (await client.getDisplayPayload(
          previewEntryId,
        )) as DisplayPayload;
        if (!isActiveSelection(token, entryId)) {
          return;
        }

        const view = await buildPreviewView(payload, t);
        if (!isActiveSelection(token, entryId)) {
          if (view.kind === "img-binary") {
            URL.revokeObjectURL(view.url);
          }
          return;
        }

        dispatch({ type: "preview/ready", token, entryId, view });
      } catch (error) {
        if (!isActiveSelection(token, entryId)) {
          return;
        }
        dispatch({
          type: "preview/error",
          token,
          entryId,
          error: errorToMessage(error),
        });
      }
    })();
  }, [t, isActiveSelection, buildPreviewView]);

  const clearSelection = () => {
    revokePreviewUrl();
    selectedEntryRef.current = null;
    dispatch({ type: "entry/clear-selection" });
  };

  const downloadSelectedEntry = async () => {
    if (!state.client || !state.selectedEntryId) {
      return;
    }

    const token = state.loadToken;
    const entryId = state.selectedEntryId;

    dispatch({ type: "download/single-start", token, entryId });

    try {
      const payload = (await state.client.getDownloadPayload(
        entryId,
      )) as DownloadPayload;
      if (!isActiveSelection(token, entryId)) {
        return;
      }

      const fileName = fallbackFileName(
        entryId,
        payload.suggested_file_name,
        payload.suggested_extension,
      );

      triggerBrowserDownload(buildImageBlob(payload), fileName);
      dispatch({ type: "download/single-success", token, entryId });
    } catch (error) {
      if (!isActiveSelection(token, entryId)) {
        return;
      }
      dispatch({
        type: "download/single-error",
        token,
        entryId,
        error: errorToMessage(error),
      });
    }
  };

  const downloadAllEntries = async () => {
    if (!state.client || state.images.length === 0) {
      return;
    }

    const token = state.loadToken;
    const client = state.client;
    const failures: BatchDownloadFailure[] = [];
    const existingNames = new Set<string>();
    const zipEntries: Record<string, Uint8Array> = {};

    dispatch({
      type: "download/batch-start",
      token,
      total: state.images.length,
    });

    try {
      let completed = 0;
      for (const image of state.images) {
        if (!isActiveSession(token)) {
          return;
        }

        try {
          const payload = (await client.getDownloadPayload(
            image.id,
          )) as DownloadPayload;
          if (!isActiveSession(token)) {
            return;
          }

          const baseName = fallbackFileName(
            image.id,
            payload.suggested_file_name,
            payload.suggested_extension,
          );
          const fileName = dedupeFileName(baseName, existingNames);
          zipEntries[fileName] = toUint8Array(payload.bytes);
        } catch (error) {
          failures.push({
            id: image.id,
            fileName: fallbackFileName(image.id, null, null),
            reason: errorToMessage(error),
          });
        } finally {
          completed += 1;
          dispatch({
            type: "download/batch-progress",
            token,
            completed,
          });
        }
      }

      if (!isActiveSession(token)) {
        return;
      }

      const archiveEntriesCount = Object.keys(zipEntries).length;
      if (archiveEntriesCount === 0) {
        toast.error(t("download.noEntries"));
        dispatch({
          type: "download/batch-error",
          token,
          error: t("download.noEntries"),
          failures,
        });
        return;
      }

      const archiveName = deriveZipName(state.fileName);
      triggerBrowserDownload(buildZipBlob(zipEntries), archiveName);

      toast.success(t("download.batchSuccess", { name: archiveName }));
      if (failures.length > 0) {
        toast.warning(t("download.batchPartial", { count: failures.length }));
      }

      dispatch({
        type: "download/batch-finish",
        token,
        failures,
        archiveName,
      });
    } catch (error) {
      if (!isActiveSession(token)) {
        return;
      }
      toast.error(t("download.batchFail", { error: errorToMessage(error) }));
      dispatch({
        type: "download/batch-error",
        token,
        error: errorToMessage(error),
        failures,
      });
    }
  };

  const onInputFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) {
      return;
    }
    void loadArchiveFile(file);
  };

  const onDragOver = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    if (!isDragOver) {
      setIsDragOver(true);
    }
  };

  const onDragLeave = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    const relatedTarget = event.relatedTarget;
    if (
      relatedTarget instanceof Node &&
      event.currentTarget.contains(relatedTarget)
    ) {
      return;
    }
    setIsDragOver(false);
  };

  const onDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setIsDragOver(false);

    const files = event.dataTransfer.files;
    if (!files || files.length === 0) {
      return;
    }

    if (files.length > 1) {
      toast.warning(t("upload.singleOnly"));
    }

    void loadArchiveFile(files[0]);
  };

  useEffect(() => {
    return () => {
      disposeThumbnailSession(sessionTokenRef.current);
      revokePreviewUrl();
      terminateActiveClient();
    };
  }, []);

  useEffect(() => {
    const updateViewport = () => {
      setIsLgViewport(window.innerWidth >= 1024);
    };

    updateViewport();
    window.addEventListener("resize", updateViewport);
    return () => {
      window.removeEventListener("resize", updateViewport);
    };
  }, []);

  const deferredSearchQuery = useDeferredValue(searchQuery);
  const loweredDeferredSearchQuery = deferredSearchQuery.trim().toLowerCase();

  const indexedImages = useMemo<SearchableImageItem[]>(() => {
    return state.images.map((image) => {
      const label = formatImageLabel(image);
      return {
        image,
        label,
        searchKeyLower: [
          image.id,
          image.facet_name,
          image.rendition_name,
          image.resolved_encoding,
        ]
          .join("\n")
          .toLowerCase(),
      };
    });
  }, [state.images]);

  const filteredImages = useMemo(() => {
    if (!loweredDeferredSearchQuery) {
      return indexedImages;
    }

    return indexedImages.filter((entry) =>
      entry.searchKeyLower.includes(loweredDeferredSearchQuery),
    );
  }, [indexedImages, loweredDeferredSearchQuery]);

  const listColumnCount = isLgViewport ? 3 : 2;
  const totalVirtualRows = Math.ceil(filteredImages.length / listColumnCount);
  const visibleRowStart = Math.max(
    0,
    Math.floor(listScrollTop / LIST_ITEM_HEIGHT) - LIST_OVERSCAN_ROWS,
  );
  const visibleRowEnd = Math.min(
    totalVirtualRows,
    Math.ceil((listScrollTop + LIST_VIEWPORT_HEIGHT) / LIST_ITEM_HEIGHT) +
      LIST_OVERSCAN_ROWS,
  );
  const visibleRows = useMemo(() => {
    const rows: SearchableImageItem[][] = [];

    for (
      let rowIndex = visibleRowStart;
      rowIndex < visibleRowEnd;
      rowIndex += 1
    ) {
      const start = rowIndex * listColumnCount;
      rows.push(filteredImages.slice(start, start + listColumnCount));
    }

    return rows;
  }, [filteredImages, listColumnCount, visibleRowEnd, visibleRowStart]);

  const ensureEntryVisible = useCallback(
    (entryId: string) => {
      const viewport = listViewportRef.current;
      if (!viewport) {
        return;
      }

      const entryIndex = filteredImages.findIndex(
        (entry) => entry.image.id === entryId,
      );
      if (entryIndex < 0) {
        return;
      }

      const rowIndex = Math.floor(entryIndex / listColumnCount);
      const rowTop = rowIndex * LIST_ITEM_HEIGHT;
      const rowBottom = rowTop + LIST_ITEM_HEIGHT;
      let nextScrollTop: number | null = null;

      if (rowTop < viewport.scrollTop) {
        nextScrollTop = rowTop;
      } else if (rowBottom > viewport.scrollTop + LIST_VIEWPORT_HEIGHT) {
        nextScrollTop = rowBottom - LIST_VIEWPORT_HEIGHT;
      }

      if (nextScrollTop == null) {
        return;
      }

      if (typeof viewport.scrollTo === "function") {
        viewport.scrollTo({ top: nextScrollTop, behavior: "smooth" });
      } else {
        viewport.scrollTop = nextScrollTop;
      }
      setListScrollTop(nextScrollTop);
    },
    [filteredImages, listColumnCount],
  );

  useEffect(() => {
    const viewport = listViewportRef.current;
    if (!viewport) {
      return;
    }

    const maxScrollTop = Math.max(
      totalVirtualRows * LIST_ITEM_HEIGHT - LIST_VIEWPORT_HEIGHT,
      0,
    );

    if (viewport.scrollTop <= maxScrollTop) {
      return;
    }

    viewport.scrollTop = maxScrollTop;
    setListScrollTop(maxScrollTop);
  }, [totalVirtualRows]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      // Ignore if user is typing in an input
      if (
        document.activeElement?.tagName === "INPUT" ||
        document.activeElement?.tagName === "TEXTAREA"
      ) {
        return;
      }

      if (filteredImages.length === 0) return;

      if (!state.selectedEntryId) {
        if (
          e.key === "ArrowRight" ||
          e.key === "ArrowDown" ||
          e.key === "ArrowLeft" ||
          e.key === "ArrowUp"
        ) {
          const firstEntryId = filteredImages[0].image.id;
          selectEntry(firstEntryId);
          ensureEntryVisible(firstEntryId);
          e.preventDefault();
        }
        return;
      }

      const currentIndex = filteredImages.findIndex(
        (img) => img.image.id === state.selectedEntryId,
      );
      if (currentIndex === -1) return;

      let nextIndex = currentIndex;
      if (e.key === "ArrowRight") {
        nextIndex = Math.min(currentIndex + 1, filteredImages.length - 1);
        e.preventDefault();
      } else if (e.key === "ArrowLeft") {
        nextIndex = Math.max(currentIndex - 1, 0);
        e.preventDefault();
      } else if (e.key === "ArrowDown") {
        nextIndex = Math.min(
          currentIndex + listColumnCount,
          filteredImages.length - 1,
        );
        e.preventDefault();
      } else if (e.key === "ArrowUp") {
        nextIndex = Math.max(currentIndex - listColumnCount, 0);
        e.preventDefault();
      }

      if (nextIndex !== currentIndex) {
        const nextEntryId = filteredImages[nextIndex].image.id;
        selectEntry(nextEntryId);
        ensureEntryVisible(nextEntryId);

        // Move browser focus to the new element
        setTimeout(() => {
          const btn = document.getElementById(`resource-card-${nextEntryId}`);
          btn?.focus({ preventScroll: true });
        }, 0);
      }
    },
    [
      ensureEntryVisible,
      filteredImages,
      state.selectedEntryId,
      state.phase,
      listColumnCount,
    ],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [handleKeyDown]);

  const selectedImage =
    state.images.find((image) => image.id === state.selectedEntryId) ?? null;
  const selectedPreviewEntryId = selectedImage
    ? resolvePreviewEntryId(selectedImage)
    : null;
  const usesDerivedPreview =
    selectedImage !== null &&
    selectedImage.preview_source_id !== null &&
    selectedImage.preview_source_id !== selectedImage.id;
  const canDownloadSelected =
    state.phase === "ready" &&
    state.selectedEntryId !== null &&
    selectedImage?.downloadable === true &&
    state.singleDownload.status !== "loading";
  const canBatchDownload =
    state.phase === "ready" &&
    state.images.length > 0 &&
    state.batchDownload.status !== "running";
  const documentEntries = Object.entries(state.documentInfo ?? {});
  const batchProgressValue =
    state.batchDownload.total > 0
      ? Math.min(state.batchDownload.completed, state.batchDownload.total)
      : 0;
  const loadProgressValue = PHASE_STEP[state.phase];

  const ThemeIcon =
    theme === "light" ? Sun : theme === "dark" ? Moon : Monitor;

  return (
    <main className="shell-frame space-y-6">
      <Toaster position="top-right" richColors />
      <header className="space-y-3">
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex min-w-0 items-center gap-3">
            <img
              src="/logo.png"
              alt=""
              aria-hidden="true"
              className="h-12 w-12 shrink-0 rounded-xl bg-white/75 p-1.5 shadow-panel ring-1 ring-border/80 dark:bg-zinc-900/75"
            />
            <h1 className="min-w-0 text-3xl font-bold tracking-tight md:text-4xl">
              {t("ui.title")}
            </h1>
          </div>
          <div className="flex shrink-0 flex-wrap items-center gap-3 sm:flex-nowrap">
            <a
              href="https://github.com/skytoup/car-parser"
              target="_blank"
              rel="noreferrer"
              aria-label={t("ui.githubRepository")}
              title={t("ui.githubRepository")}
              className={cn(
                buttonVariants({ variant: "outline", size: "sm" }),
                "h-8 gap-1.5 rounded-full bg-background/50 px-3 text-xs shadow-sm backdrop-blur-md",
              )}
            >
              <ExternalLink className="h-4 w-4" aria-hidden="true" />
              <span>{t("ui.githubRepositoryAction")}</span>
            </a>

            <div className="relative inline-flex items-center group">
              <select
                className="appearance-none border border-input bg-background/50 hover:bg-background backdrop-blur-md rounded-full text-xs pl-8 pr-7 py-1 shadow-sm focus:outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary transition-all cursor-pointer font-medium text-foreground/80 relative z-0"
                value={theme}
                onChange={(e) => setTheme(e.target.value as Theme)}
              >
                <option value="light">{t("ui.light")}</option>
                <option value="dark">{t("ui.dark")}</option>
                <option value="auto">{t("ui.auto")}</option>
              </select>
              <ThemeIcon className="absolute left-2.5 h-3.5 w-3.5 text-foreground/50 pointer-events-none z-10" />
              <ChevronDown className="absolute right-2.5 h-3.5 w-3.5 text-foreground/50 pointer-events-none z-10" />
            </div>

            <div className="relative inline-flex items-center group">
              <select
                className="appearance-none border border-input bg-background/50 hover:bg-background backdrop-blur-md rounded-full text-xs pl-8 pr-7 py-1 shadow-sm focus:outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary transition-all cursor-pointer font-medium text-foreground/80 relative z-0"
                value={i18n.language}
                onChange={(e) => i18n.changeLanguage(e.target.value)}
              >
                <option value="zh">{t("ui.zh")}</option>
                <option value="en">{t("ui.en")}</option>
              </select>
              <Languages className="absolute left-2.5 h-3.5 w-3.5 text-foreground/50 pointer-events-none z-10" />
              <ChevronDown className="absolute right-2.5 h-3.5 w-3.5 text-foreground/50 pointer-events-none z-10" />
            </div>
          </div>
        </div>
      </header>

      <Card>
        <CardHeader>
          <CardDescription>{t("ui.desc")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <input
            ref={fileInputRef}
            type="file"
            accept=".car,application/octet-stream"
            className="hidden"
            aria-label={t("ui.selectFile")}
            onChange={onInputFileChange}
          />

          <div
            onDragOver={onDragOver}
            onDragLeave={onDragLeave}
            onDrop={onDrop}
            className={[
              "rounded-xl border-2 border-dashed p-8 text-center transition-all duration-300",
              isDragOver
                ? "border-primary bg-primary/5 scale-[1.01] shadow-inner drag-pulse drag-dash"
                : "border-muted-foreground/20 bg-muted/5 hover:border-primary/40 hover:bg-muted/10",
            ].join(" ")}
          >
            <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-primary/10 text-primary">
              <Inbox className="h-6 w-6" />
            </div>
            <p className="text-base font-semibold text-foreground/90">{t("ui.dragDrop")}</p>
            <p className="mt-1 text-sm text-muted-foreground/80">
              {t("ui.orClick")}
            </p>
            <Button
              variant="outline"
              size="sm"
              className="mt-4 rounded-full px-6"
              onClick={() => fileInputRef.current?.click()}
            >
              {t("ui.selectFile")}
            </Button>
          </div>

          <div className="flex flex-wrap items-center gap-3 text-sm">
            {state.phase === "idle" ? (
              <></>
            ) : (
              <Badge
                variant={state.phase === "error" ? "destructive" : "secondary"}
              >
                {t(`phase.${state.phase}`)}
              </Badge>
            )}
            {state.fileName ? (
              <span className="font-medium text-foreground/90">
                {t("ui.currentFile", { name: state.fileName })}
              </span>
            ) : (
              <></>
            )}
          </div>

          {state.phase === "idle" ? (
            <></>
          ) : (
            <div className="space-y-2">
              <Progress value={loadProgressValue} max={4} />
            </div>
          )}

          {state.loadError ? (
            <div className="rounded-lg border border-destructive/25 bg-destructive/10 p-3 text-sm text-destructive">
              {state.loadError}
            </div>
          ) : null}
        </CardContent>
      </Card>

      {state.phase !== "idle" && (
        <>
          <Card>
            <CardHeader
              className="space-y-2 cursor-pointer hover:bg-muted/30 transition-colors"
              onClick={() => setIsDocInfoExpanded(!isDocInfoExpanded)}
            >
              <div className="flex items-center justify-between">
                <CardTitle>{t("ui.docInfo")}</CardTitle>
                <Button variant="ghost" size="icon" className="h-8 w-8">
                  {isDocInfoExpanded ? (
                    <ChevronUp className="h-4 w-4" />
                  ) : (
                    <ChevronDown className="h-4 w-4" />
                  )}
                </Button>
              </div>
            </CardHeader>
            {isDocInfoExpanded && (
              <CardContent>
                {documentEntries.length === 0 ? (
                  <div className="flex flex-col items-center justify-center space-y-3 py-8 text-muted-foreground">
                    <FileX className="h-8 w-8 opacity-20" />
                    <p className="text-sm">{t("ui.noDocInfo")}</p>
                  </div>
                ) : (
                  <dl className="grid gap-x-4 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 text-sm">
                    {documentEntries.map(([key, value]) => (
                      <div
                        key={key}
                        className="flex flex-col border-b border-border/40 py-2.5"
                      >
                        <dt className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                          {key}
                        </dt>
                        <dd className="mt-0.5 break-all font-semibold text-foreground/90">
                          {formatDocumentValue(value)}
                        </dd>
                      </div>
                    ))}
                  </dl>
                )}
              </CardContent>
            )}
          </Card>

          {state.batchDownload.failures.length > 0 ? (
            <Card>
              <CardHeader>
                <CardTitle>{t("ui.batchFailTitle")}</CardTitle>
                <CardDescription>
                  {t("ui.batchFailDesc", {
                    count: state.batchDownload.failures.length,
                  })}
                </CardDescription>
              </CardHeader>
              <CardContent>
                <ul className="max-h-56 space-y-2 overflow-y-auto text-sm">
                  {state.batchDownload.failures.map((failure) => (
                    <li
                      key={`${failure.id}:${failure.fileName}`}
                      className="rounded-lg border border-border/70 p-3"
                    >
                      <p className="font-semibold">{failure.fileName}</p>
                      <p className="mt-1 break-all text-xs text-muted-foreground">
                        ID: {failure.id}
                      </p>
                      <p className="mt-1 text-xs text-destructive">
                        {failure.reason}
                      </p>
                    </li>
                  ))}
                </ul>
              </CardContent>
            </Card>
          ) : null}

          <section className="grid gap-6 lg:grid-cols-2 items-start">
            <Card className="h-fit flex flex-col">
              <CardHeader className="space-y-4 pb-3">
                <div className="flex items-center justify-between gap-4">
                  <div className="flex items-baseline gap-2 min-w-0">
                    <CardTitle className="whitespace-nowrap">
                      {t("ui.resourceList")}
                    </CardTitle>
                    <CardDescription className="truncate">
                      {t("ui.resourceListDesc", { count: state.images.length })}
                    </CardDescription>
                  </div>
                  <Button
                    variant="default"
                    size="sm"
                    className="h-8"
                    onClick={() => {
                      void downloadAllEntries();
                    }}
                    disabled={!canBatchDownload}
                  >
                    {state.batchDownload.status === "running" && (
                      <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
                    )}
                    {state.batchDownload.status === "running"
                      ? t("download.running")
                      : t("download.batchAll")}
                  </Button>
                </div>

                {state.batchDownload.status === "running" ? (
                  <div className="space-y-1.5 border-t pt-3">
                    <Progress
                      value={batchProgressValue}
                      max={state.batchDownload.total || 1}
                      className="h-1.5"
                    />
                    <p className="text-[10px] text-muted-foreground flex justify-between">
                      <span>
                        {t("ui.progress", {
                          completed: state.batchDownload.completed,
                          total: state.batchDownload.total,
                        })}
                      </span>
                      {state.batchDownload.archiveName && (
                        <span className="truncate max-w-[150px]">
                          {state.batchDownload.archiveName}
                        </span>
                      )}
                    </p>
                  </div>
                ) : null}
                {state.batchDownload.status === "success" ? (
                  <p className="text-xs font-medium text-emerald-600 dark:text-emerald-400 bg-emerald-500/5 p-3 rounded-r-lg border-l-4 border-emerald-500/50">
                    {t("download.batchSuccess", {
                      name: state.batchDownload.archiveName || "",
                    })}
                  </p>
                ) : null}
                {state.batchDownload.status === "partial-failure" ? (
                  <p className="text-xs font-medium text-amber-600 dark:text-amber-400 bg-amber-500/5 p-3 rounded-r-lg border-l-4 border-amber-500/50">
                    {t("download.batchPartial", {
                      count: state.batchDownload.failures.length,
                    })}
                  </p>
                ) : null}
                {state.batchDownload.status === "error" ? (
                  <p className="text-xs font-medium text-destructive bg-destructive/5 p-3 rounded-r-lg border-l-4 border-destructive/50">
                    {t("download.batchFail", {
                      error: state.batchDownload.error || "",
                    })}
                  </p>
                ) : null}
              </CardHeader>
              <CardContent className="space-y-3 pt-0">
                {state.phase !== "ready" ? (
                  <div className="flex flex-col items-center justify-center space-y-3 py-16 text-muted-foreground">
                    <Inbox className="h-12 w-12 opacity-20 animate-pulse" />
                    <p className="text-sm">{t("ui.loadingList")}</p>
                  </div>
                ) : null}
                {state.phase === "ready" && state.images.length === 0 ? (
                  <div className="flex flex-col items-center justify-center space-y-3 py-16 text-muted-foreground">
                    <ImageOff className="h-12 w-12 opacity-20" />
                    <p className="text-sm">{t("ui.noImages")}</p>
                  </div>
                ) : null}
                {state.phase === "ready" && state.images.length > 0 ? (
                  <>
                    <div className="relative px-4 pb-2">
                      <Search className="absolute left-[26px] top-2.5 h-4 w-4 text-muted-foreground" />
                      <input
                        type="text"
                        placeholder={t("ui.searchPlaceholder")}
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        className="flex h-9 w-full rounded-md border border-input bg-background/30 px-9 py-1 text-sm shadow-sm transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/20 focus-visible:border-primary/50 focus:bg-background/50"
                      />
                    </div>
                    {filteredImages.length === 0 ? (
                      <div className="flex flex-col items-center justify-center space-y-3 py-10 text-muted-foreground">
                        <Search className="h-8 w-8 opacity-20" />
                        <p className="text-sm">
                          {t("ui.noMatch", { query: deferredSearchQuery })}
                        </p>
                      </div>
                    ) : (
                      <div
                        ref={listViewportRef}
                        data-testid="resource-list-viewport"
                        className="h-[560px] overflow-y-auto px-4 pt-2 scrollbar-thin scrollbar-thumb-muted-foreground/20 scrollbar-track-transparent hover:scrollbar-thumb-muted-foreground/30 pb-2"
                        onScroll={(event) =>
                          setListScrollTop(event.currentTarget.scrollTop)
                        }
                      >
                        <div
                          className="relative"
                          style={{
                            height: `${totalVirtualRows * LIST_ITEM_HEIGHT}px`,
                          }}
                        >
                          {visibleRows.map((rowItems, visibleRowIndex) => {
                            const rowIndex = visibleRowStart + visibleRowIndex;
                            return (
                              <div
                                key={`row-${rowIndex}`}
                                className={`absolute inset-x-0 grid gap-4 pb-4 ${listColumnCount === 3 ? "grid-cols-3" : "grid-cols-2"}`}
                                style={{
                                  top: `${rowIndex * LIST_ITEM_HEIGHT}px`,
                                  height: `${LIST_ITEM_HEIGHT}px`,
                                }}
                              >
                                {rowItems.map(({ image, label }) => {
                                  const isActive =
                                    state.selectedEntryId === image.id;
                                  return (
                                    <ResourceCard
                                      key={image.id}
                                      image={image}
                                      label={label}
                                      isActive={isActive}
                                      onSelect={selectEntry}
                                      client={state.client}
                                      loadToken={state.loadToken}
                                      searchQuery={deferredSearchQuery}
                                    />
                                  );
                                })}
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    )}
                  </>
                ) : null}
              </CardContent>
            </Card>

            <div className="lg:sticky lg:top-6 flex flex-col p-1 -m-1">
              <Card className="flex-1 flex flex-col lg:max-h-[calc(100vh-3rem)]">
                <CardHeader className="space-y-2">
                  <CardTitle>{t("ui.detailTitle")}</CardTitle>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="default"
                      onClick={() => {
                        void downloadSelectedEntry();
                      }}
                      disabled={!canDownloadSelected}
                    >
                      {state.singleDownload.status === "loading" && (
                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      )}
                      {state.singleDownload.status === "loading"
                        ? t("download.singleLoading")
                        : t("download.singleDownload")}
                    </Button>
                    <Button
                      variant="outline"
                      onClick={() => {
                        if (state.selectedEntryId) {
                          selectEntry(state.selectedEntryId, { force: true });
                        }
                      }}
                      disabled={
                        !state.selectedEntryId ||
                        state.preview.status === "loading"
                      }
                    >
                      {t("ui.retryPreview")}
                    </Button>
                    <Button
                      variant="outline"
                      onClick={clearSelection}
                      disabled={!state.selectedEntryId}
                    >
                      {t("ui.clearSelection")}
                    </Button>
                  </div>
                  {state.singleDownload.status === "error" &&
                  state.singleDownload.error ? (
                    <p className="text-xs text-destructive">
                      {t("download.singleFail", {
                        error: state.singleDownload.error,
                      })}
                    </p>
                  ) : null}
                </CardHeader>
                <CardContent className="flex-1 lg:overflow-y-auto space-y-4 min-h-[400px] scrollbar-thin scrollbar-thumb-muted-foreground/20 scrollbar-track-transparent hover:scrollbar-thumb-muted-foreground/30 pb-6">
                  {!state.selectedEntryId ? (
                    <div className="flex flex-col items-center justify-center space-y-3 py-12 text-muted-foreground">
                      <ImageOff className="h-12 w-12 opacity-15" />
                      <p className="text-sm">{t("preview.noSelection")}</p>
                    </div>
                  ) : null}

                  {state.selectedEntryId ? (
                    <div className="rounded-xl border border-border/70 bg-card/70 p-3">
                      <p className="text-xs text-muted-foreground">
                        {t("ui.currentId")}
                      </p>
                      <p className="mt-1 break-all font-mono text-sm font-semibold">
                        {state.selectedEntryId}
                      </p>
                    </div>
                  ) : null}

                  {usesDerivedPreview && selectedPreviewEntryId ? (
                    <div className="rounded-r-xl border-l-4 border-sky-500/50 bg-sky-500/5 p-4 text-sm font-medium text-sky-600 dark:text-sky-400">
                      {t("preview.derived", {
                        previewId: selectedPreviewEntryId,
                      })}
                    </div>
                  ) : null}

                  {state.preview.status === "loading" ? (
                    <div className="flex flex-col items-center justify-center space-y-4 rounded-xl border border-border/40 bg-muted/5 p-12 text-sm text-muted-foreground shadow-inner">
                      <Loader2 className="h-8 w-8 animate-spin text-primary/40" />
                      <p>{t("preview.loading")}</p>
                    </div>
                  ) : null}

                  {state.preview.status === "error" ? (
                    <div className="rounded-r-xl border-l-4 border-destructive/50 bg-destructive/5 p-4 text-sm font-medium text-destructive">
                      {t("preview.error", { error: state.preview.error || "" })}
                    </div>
                  ) : null}

                  {state.preview.status === "ready" &&
                  state.preview.view.kind === "img-binary" ? (
                    <div className="rounded-xl border border-border/50 preview-checkerboard flex justify-center items-center overflow-hidden shadow-sm">
                      <Zoom>
                        <img
                          src={state.preview.view.url}
                          alt={
                            selectedImage
                              ? formatImageLabel(selectedImage)
                              : "CAR resource preview"
                          }
                          className="max-h-[560px] w-full object-contain"
                        />
                      </Zoom>
                    </div>
                  ) : null}

                  {state.preview.status === "ready" &&
                  state.preview.view.kind === "canvas-rgba" ? (
                    <div className="rounded-xl border border-border/50 preview-checkerboard flex justify-center items-center overflow-hidden shadow-sm">
                      <CanvasPreview
                        width={state.preview.view.width}
                        height={state.preview.view.height}
                        rgba={state.preview.view.rgba}
                        className="max-h-[560px] w-full object-contain"
                      />
                    </div>
                  ) : null}

                  {state.preview.status === "ready" &&
                  state.preview.view.kind === "color-swatch" ? (
                    <div className="rounded-xl border border-border/50 bg-card p-4 space-y-4 shadow-inner ring-1 ring-border/5">
                      <div className="rounded-xl border border-border/50 overflow-hidden">
                        <div className="h-56 bg-[linear-gradient(45deg,rgba(148,163,184,0.12)_25%,transparent_25%,transparent_75%,rgba(148,163,184,0.12)_75%),linear-gradient(45deg,rgba(148,163,184,0.12)_25%,transparent_25%,transparent_75%,rgba(148,163,184,0.12)_75%)] bg-[length:28px_28px] bg-[position:0_0,14px_14px] p-4">
                          <div
                            className="h-full w-full rounded-lg border border-white/40 shadow-inner"
                            style={{ backgroundColor: state.preview.view.cssColor }}
                          />
                        </div>
                      </div>
                      <dl className="grid gap-x-4 sm:grid-cols-2 text-sm bg-muted/20 rounded-lg p-3 border border-border/40">
                        <div className="flex flex-col border-b border-border/40 py-2.5">
                          <dt className="text-[11px] text-muted-foreground uppercase tracking-wide font-medium">
                            Color Space
                          </dt>
                          <dd className="mt-0.5 break-all font-semibold text-foreground/90">
                            {state.preview.view.colorSpace}
                          </dd>
                        </div>
                        <div className="flex flex-col border-b border-border/40 py-2.5">
                          <dt className="text-[11px] text-muted-foreground uppercase tracking-wide font-medium">
                            CSS
                          </dt>
                          <dd className="mt-0.5 break-all font-semibold text-foreground/90">
                            {state.preview.view.cssColor}
                          </dd>
                        </div>
                        <div className="flex flex-col border-b border-border/40 py-2.5 sm:col-span-2">
                          <dt className="text-[11px] text-muted-foreground uppercase tracking-wide font-medium">
                            Components
                          </dt>
                          <dd className="mt-0.5 break-all font-semibold text-foreground/90">
                            {state.preview.view.components.join(", ")}
                          </dd>
                        </div>
                      </dl>
                    </div>
                  ) : null}

                  {state.preview.status === "ready" &&
                  state.preview.view.kind === "none" ? (
                    <div className="rounded-r-xl border-l-4 border-amber-500/50 bg-amber-500/5 p-4 text-sm font-medium text-amber-700 dark:text-amber-500">
                      {state.preview.view.message}
                    </div>
                  ) : null}

                  {state.selectedEntryInfo ? (
                    <div className="space-y-4">
                      <div className="space-y-2">
                        <h4 className="text-sm font-semibold flex items-center text-foreground/80">
                          {t("ui.basicInfo")}
                        </h4>
                        <dl className="grid gap-x-4 sm:grid-cols-2 text-sm bg-muted/20 rounded-lg p-3 border border-border/40">
                          {[
                            [
                              "Facet",
                              state.selectedEntryInfo.facet_name || "-",
                            ],
                            [
                              "Rendition",
                              state.selectedEntryInfo.rendition_name || "-",
                            ],
                            [
                              "Size",
                              `${state.selectedEntryInfo.width}x${state.selectedEntryInfo.height}`,
                            ],
                            [
                              "Suggested Name",
                              state.selectedEntryInfo.suggested_file_name || "-",
                              true,
                            ],
                            ...(state.selectedEntryInfo.preview_source_id &&
                            state.selectedEntryInfo.preview_source_id !==
                              state.selectedEntryInfo.id
                              ? [
                                  [
                                    "Preview Source",
                                    state.selectedEntryInfo.preview_source_id,
                                    true,
                                  ] as const,
                                ]
                              : []),
                          ].map(([label, value, isFullWidth]) => (
                            <div
                              key={label.toString()}
                              className={`flex flex-col border-b border-border/40 py-2.5 ${isFullWidth ? "sm:col-span-2" : ""}`}
                            >
                              <dt className="text-[11px] text-muted-foreground uppercase tracking-wide font-medium">
                                {label}
                              </dt>
                              <dd className="mt-0.5 break-all font-semibold text-foreground/90">
                                {value as React.ReactNode}
                              </dd>
                            </div>
                          ))}
                        </dl>
                      </div>

                      <div className="space-y-2">
                        <button
                          type="button"
                          onClick={() =>
                            setIsAdvancedParamsExpanded(
                              !isAdvancedParamsExpanded,
                            )
                          }
                          className="text-sm font-semibold flex items-center text-foreground/80 hover:text-primary transition-colors w-full"
                        >
                          {isAdvancedParamsExpanded ? (
                            <ChevronDown className="h-4 w-4 mr-1" />
                          ) : (
                            <ChevronRight className="h-4 w-4 mr-1" />
                          )}
                          {t("ui.advancedInfo")}
                        </button>
                        {isAdvancedParamsExpanded && (
                          <dl className="grid gap-x-4 sm:grid-cols-2 text-sm bg-muted/20 rounded-lg p-3 border border-border/40">
                            {[
                              ["Kind", state.selectedEntryInfo.entry_kind],
                              ["Scale", state.selectedEntryInfo.scale],
                              [
                                "Encoding",
                                state.selectedEntryInfo.resolved_encoding,
                              ],
                              [
                                "Layout",
                                state.selectedEntryInfo.logical_layout,
                              ],
                              [
                                "Preview Strategy",
                                state.selectedEntryInfo.preview_strategy,
                              ],
                              [
                                "Download Strategy",
                                state.selectedEntryInfo.download_strategy,
                              ],
                              ["Mime", state.selectedEntryInfo.mime_type],
                              [
                                "Selection Reason",
                                state.selectedEntryInfo.selection_reason,
                              ],
                            ].map(([label, value, isFullWidth]) => (
                              <div
                                key={label.toString()}
                                className={`flex flex-col border-b border-border/40 py-2.5 ${isFullWidth ? "sm:col-span-2" : ""}`}
                              >
                                <dt className="text-[11px] text-muted-foreground uppercase tracking-wide font-medium">
                                  {label}
                                </dt>
                                <dd className="mt-0.5 break-all font-semibold text-foreground/90">
                                  {value as React.ReactNode}
                                </dd>
                              </div>
                            ))}
                          </dl>
                        )}
                      </div>
                    </div>
                  ) : null}

                  {state.detailError ? (
                    <div className="rounded-xl border border-destructive/25 bg-destructive/10 p-4 text-sm text-destructive">
                      {t("ui.metaFail", { error: state.detailError })}
                    </div>
                  ) : null}
                </CardContent>
              </Card>
            </div>
          </section>
        </>
      )}
    </main>
  );
}

export default App;
