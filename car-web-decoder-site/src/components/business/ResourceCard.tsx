import { memo } from "react";
import { Image as ImageIcon, FileText, Download, Check, Palette, FileDigit } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { LazyImagePreview } from "./LazyImagePreview";
import type { CarArchiveClient, ImageListItem } from "@/lib/types";

interface ResourceCardProps {
  image: ImageListItem;
  label: string;
  isActive: boolean;
  onSelect: (id: string) => void;
  client: CarArchiveClient | null;
  loadToken: number;
  searchQuery: string;
}

function HighlightText({ text, query }: { text: string; query: string }) {
  if (!query.trim()) return <>{text}</>;
  const parts = text.split(new RegExp(`(${query})`, "gi"));
  return (
    <>
      {parts.map((part, i) =>
        part.toLowerCase() === query.toLowerCase() ? (
          <mark
            key={i}
            className="bg-primary/25 text-primary-foreground dark:text-primary rounded-sm px-[2px] font-bold"
          >
            {part}
          </mark>
        ) : (
          part
        ),
      )}
    </>
  );
}

export const ResourceCard = memo(function ResourceCard({
  image,
  label,
  isActive,
  onSelect,
  client,
  loadToken,
  searchQuery,
}: ResourceCardProps) {
  const hasInlinePreview = image.preview_source_id !== null;
  const usesDerivedPreview =
    image.preview_source_id !== null && image.preview_source_id !== image.id;

  const getPreviewIcon = () => {
    if (image.entry_kind === "color") {
      return <Palette className="h-3 w-3" />;
    }
    if (image.entry_kind === "raw-data") {
      return <FileDigit className="h-3 w-3" />;
    }
    if (hasInlinePreview) {
      return <ImageIcon className="h-3 w-3" />;
    }
    if (image.preview_strategy === "document") {
      return <FileText className="h-3 w-3" />;
    }
    return <Download className="h-3 w-3" />;
  };

  const previewBadgeText = () => {
    if (usesDerivedPreview) {
      return "derived";
    }
    if (hasInlinePreview) {
      return "preview";
    }
    return image.preview_strategy;
  };

  const renderCover = () => {
    if (image.entry_kind === "color" && image.css_color) {
      return (
        <div className="flex h-full w-full items-center justify-center p-4">
          <div
            className="h-full w-full rounded-lg border border-white/40 shadow-sm"
            style={{ backgroundColor: image.css_color }}
          />
        </div>
      );
    }

    if (client) {
      return (
        <LazyImagePreview
          client={client}
          image={image}
          loadToken={loadToken}
          maxDimension={256}
        />
      );
    }

    return null;
  };

  return (
    <div
      id={`list-item-${image.id}`}
      className="relative h-full group"
    >
      {isActive && (
        <div className="absolute -left-1 top-4 bottom-4 w-1.5 bg-primary rounded-r-full z-10 shadow-[0_0_8px_hsl(var(--primary)/0.4)] transition-all duration-200 animate-in fade-in slide-in-from-left-1" />
      )}
      <button
        id={`resource-card-${image.id}`}
        type="button"
        onClick={() => onSelect(image.id)}
        className={[
          "flex h-full w-full flex-col rounded-xl border text-left transition-[background-color,shadow,transform] duration-200 overflow-hidden outline-none",
          isActive
            ? "border-primary bg-primary/5 shadow-md ring-1 ring-primary"
            : "border-border/60 hover:border-primary/30 hover:shadow-md bg-muted/30 hover:bg-muted/50 dark:bg-zinc-900/40 dark:hover:bg-zinc-800/60",
        ].join(" ")}
      >
        <div className="relative w-full aspect-[4/3] min-h-[120px] bg-muted/20 bg-[linear-gradient(45deg,rgba(0,0,0,0.02)_25%,transparent_25%,transparent_75%,rgba(0,0,0,0.02)_75%),linear-gradient(45deg,rgba(0,0,0,0.02)_25%,transparent_25%,transparent_75%,rgba(0,0,0,0.02)_75%)] bg-[length:24px_24px] bg-[position:0_0,12px_12px] border-b border-border/10 shadow-inner">
          {renderCover()}
          <div className="absolute top-2 right-2 flex gap-1">
            <Badge
              variant={"secondary"}
              className="h-5 px-1.5 text-[10px] gap-1 backdrop-blur-md bg-background/70 border-none shadow-sm"
            >
              {getPreviewIcon()}
            </Badge>
            {usesDerivedPreview && (
              <Badge
                variant="outline"
                className="h-5 px-1.5 text-[10px] backdrop-blur-md bg-background/40 border-border/20"
              >
                raw
              </Badge>
            )}
          </div>
        </div>

        <div className="flex flex-1 flex-col p-3 w-full relative min-h-0">
          <div className="flex items-start justify-between gap-2 mb-1">
            <p
              className="text-sm font-bold truncate leading-tight flex-1 text-foreground"
              title={label}
            >
              <HighlightText
                text={label}
                query={searchQuery}
              />
            </p>
          </div>
          
          <p className="text-[10px] font-mono text-muted-foreground truncate mb-2" title={image.id}>
            ID: <HighlightText text={image.id} query={searchQuery} />
          </p>

          <div className="mt-auto space-y-1.5">
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="text-[11px] font-medium text-foreground/80 bg-muted/60 px-1.5 py-0.5 rounded border border-border/50">
                {image.width}×{image.height}
                {image.scale !== 1 && (
                  <span className="ml-1 opacity-60">@{image.scale}x</span>
                )}
              </span>
                <Badge
                  variant="outline"
                  className="text-[10px] px-1.5 py-0 font-normal border-border/50 bg-muted/30 text-muted-foreground"
                >
                  <HighlightText
                    text={image.resolved_encoding}
                    query={searchQuery}
                  />
                </Badge>
                <Badge
                  variant="outline"
                  className="text-[10px] px-1.5 py-0 font-normal border-border/50 bg-muted/30 text-muted-foreground"
                >
                  {image.entry_kind}
                </Badge>
                {!image.downloadable && (
                  <Badge
                    variant="secondary"
                    className="text-[10px] px-1.5 py-0"
                  >
                    metadata
                  </Badge>
                )}
              </div>
            </div>
          </div>
      </button>
    </div>
  );
});
;


;

