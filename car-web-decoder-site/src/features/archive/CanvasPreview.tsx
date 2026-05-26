import { useLayoutEffect, useEffect, useRef, forwardRef, useImperativeHandle, memo } from "react";

import { toUint8ClampedArray } from "@/features/archive/archive-utils";

interface CanvasPreviewProps {
  width: number;
  height: number;
  rgba: Uint8ClampedArray;
  className?: string;
}

export const CanvasPreview = memo(forwardRef<HTMLCanvasElement, CanvasPreviewProps>(
  ({ width, height, rgba, className }, ref) => {
    const canvasRef = useRef<HTMLCanvasElement | null>(null);

    // Expose the internal canvas element to the forwarded ref
    useImperativeHandle(ref, () => canvasRef.current!);

    useLayoutEffect(() => {
      const canvas = canvasRef.current;
      if (!canvas) {
        return;
      }

      const context = canvas.getContext("2d");
      if (!context) {
        return;
      }

      const ownedRgba = toUint8ClampedArray(rgba) as Uint8ClampedArray<ArrayBuffer>;
      const imageData = new ImageData(ownedRgba, width, height);
      context.clearRect(0, 0, width, height);
      context.putImageData(imageData, 0, 0);
    }, [height, rgba, width]);

    return (
      <canvas
        ref={canvasRef}
        width={width}
        height={height}
        className={className ?? "max-h-[480px] w-full rounded border border-border bg-card object-contain"}
      />
    );
  }
));

CanvasPreview.displayName = "CanvasPreview";
