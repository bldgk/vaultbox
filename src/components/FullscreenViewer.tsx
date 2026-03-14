import { useEffect, useMemo, useCallback, useState, useRef } from "react";
import { useFileStore } from "../store/fileStore";
import { getViewerType } from "../lib/fileTypes";

const PREVIEWABLE_TYPES = new Set(["image", "media"]);

function getMediaUrl(filePath: string): string {
  return `vaultmedia://localhost/${encodeURIComponent(filePath)}`;
}

export function FullscreenViewer() {
  const { fullscreenPreview, setFullscreenPreview, entries, currentPath } = useFileStore();
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const dragStart = useRef({ x: 0, y: 0, panX: 0, panY: 0 });
  const videoRef = useRef<HTMLVideoElement>(null);

  // Get list of previewable files in current directory
  const previewableFiles = useMemo(() => {
    return entries
      .filter((e) => !e.is_dir && PREVIEWABLE_TYPES.has(getViewerType(e.name)))
      .map((e) => ({
        name: e.name,
        path: currentPath ? `${currentPath}/${e.name}` : e.name,
      }));
  }, [entries, currentPath]);

  const currentIndex = useMemo(() => {
    if (!fullscreenPreview) return -1;
    return previewableFiles.findIndex((f) => f.path === fullscreenPreview.filePath);
  }, [fullscreenPreview, previewableFiles]);

  const navigateToFile = useCallback((index: number) => {
    if (index < 0 || index >= previewableFiles.length) return;
    const file = previewableFiles[index];
    setZoom(1);
    setPan({ x: 0, y: 0 });
    // Images/videos use vaultmedia:// — we only need the path, no content download.
    // Pass a dummy content object since FullscreenPreview requires it.
    setFullscreenPreview({
      filePath: file.path,
      fileName: file.name,
      content: { type: "Binary", data: "" },
    });
  }, [previewableFiles, setFullscreenPreview]);

  const goNext = useCallback(() => {
    if (currentIndex < previewableFiles.length - 1) navigateToFile(currentIndex + 1);
  }, [currentIndex, previewableFiles.length, navigateToFile]);

  const goPrev = useCallback(() => {
    if (currentIndex > 0) navigateToFile(currentIndex - 1);
  }, [currentIndex, navigateToFile]);

  // Derive viewer type and sources (must be before early return so hooks are stable)
  const viewerType = fullscreenPreview ? getViewerType(fullscreenPreview.fileName) : null;
  const isImage = viewerType === "image";
  const isVideo = viewerType === "media";

  // Both images and videos use vaultmedia:// — no base64, no blob URLs, instant
  const mediaSrc = useMemo(() => {
    if (!fullscreenPreview || (!isImage && !isVideo)) return null;
    return getMediaUrl(fullscreenPreview.filePath);
  }, [fullscreenPreview, isImage, isVideo]);

  // Keyboard handler
  useEffect(() => {
    if (!fullscreenPreview) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setFullscreenPreview(null);
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        goNext();
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        goPrev();
      } else if (e.key === "+" || e.key === "=") {
        setZoom((z) => Math.min(z * 1.3, 10));
      } else if (e.key === "-") {
        setZoom((z) => Math.max(z / 1.3, 0.1));
      } else if (e.key === "0") {
        setZoom(1);
        setPan({ x: 0, y: 0 });
      } else if (e.key === " " && videoRef.current) {
        e.preventDefault();
        if (videoRef.current.paused) videoRef.current.play();
        else videoRef.current.pause();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [fullscreenPreview, goNext, goPrev, setFullscreenPreview]);

  // Mouse wheel zoom
  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setZoom((z) => Math.min(Math.max(z * delta, 0.1), 10));
  }, []);

  // Pan with mouse drag
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (zoom <= 1) return;
    setIsDragging(true);
    dragStart.current = { x: e.clientX, y: e.clientY, panX: pan.x, panY: pan.y };
  }, [zoom, pan]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isDragging) return;
    setPan({
      x: dragStart.current.panX + (e.clientX - dragStart.current.x),
      y: dragStart.current.panY + (e.clientY - dragStart.current.y),
    });
  }, [isDragging]);

  const handleMouseUp = useCallback(() => setIsDragging(false), []);

  if (!fullscreenPreview) return null;

  const hasPrev = currentIndex > 0;
  const hasNext = currentIndex < previewableFiles.length - 1;

  return (
    <div className="fixed inset-0 z-[100] bg-black flex flex-col select-none">
      {/* Top bar */}
      <div className="flex items-center justify-between px-4 py-2 bg-black/80 text-white z-10">
        <span className="text-sm text-gray-300 truncate max-w-[50%]">
          {fullscreenPreview.fileName}
          {previewableFiles.length > 1 && (
            <span className="text-gray-500 ml-2">
              {currentIndex + 1} / {previewableFiles.length}
            </span>
          )}
        </span>
        <div className="flex items-center gap-2">
          {isImage && (
            <>
              <button onClick={() => setZoom((z) => Math.max(z / 1.3, 0.1))} className="p-1.5 rounded hover:bg-white/10 text-gray-300" title="Zoom out">
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 12H4" /></svg>
              </button>
              <span className="text-xs text-gray-400 w-12 text-center">{Math.round(zoom * 100)}%</span>
              <button onClick={() => setZoom((z) => Math.min(z * 1.3, 10))} className="p-1.5 rounded hover:bg-white/10 text-gray-300" title="Zoom in">
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" /></svg>
              </button>
              <button onClick={() => { setZoom(1); setPan({ x: 0, y: 0 }); }} className="p-1.5 rounded hover:bg-white/10 text-gray-300 text-xs" title="Reset zoom">
                Fit
              </button>
              <div className="w-px h-5 bg-gray-700 mx-1" />
            </>
          )}
          <button
            onClick={() => setFullscreenPreview(null)}
            className="p-1.5 rounded hover:bg-white/10 text-gray-300"
            title="Close (Esc)"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>

      {/* Content area */}
      <div
        className="flex-1 flex items-center justify-center overflow-hidden relative"
        onWheel={isImage ? handleWheel : undefined}
        onMouseDown={isImage ? handleMouseDown : undefined}
        onMouseMove={isImage ? handleMouseMove : undefined}
        onMouseUp={isImage ? handleMouseUp : undefined}
        onMouseLeave={isImage ? handleMouseUp : undefined}
        style={{ cursor: isImage && zoom > 1 ? (isDragging ? "grabbing" : "grab") : "default" }}
      >
        {isImage && mediaSrc && (
          <img
            src={mediaSrc}
            alt={fullscreenPreview.fileName}
            className="transition-transform duration-100"
            style={{
              transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
              maxWidth: zoom <= 1 ? "100%" : "none",
              maxHeight: zoom <= 1 ? "100%" : "none",
            }}
            draggable={false}
            onDoubleClick={() => {
              if (zoom === 1) {
                setZoom(2);
              } else {
                setZoom(1);
                setPan({ x: 0, y: 0 });
              }
            }}
          />
        )}
        {isVideo && mediaSrc && (
          <video
            ref={videoRef}
            controls
            autoPlay
            src={mediaSrc}
            className="max-w-full max-h-full"
          />
        )}
        {!mediaSrc && (
          <div className="text-gray-500">Cannot display preview</div>
        )}
      </div>

      {/* Navigation arrows */}
      {hasPrev && (
        <button
          onClick={goPrev}
          className="absolute left-4 top-1/2 -translate-y-1/2 p-3 rounded-full bg-black/50 hover:bg-black/80 text-white/70 hover:text-white transition z-10"
          title="Previous (Left arrow)"
        >
          <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
        </button>
      )}
      {hasNext && (
        <button
          onClick={goNext}
          className="absolute right-4 top-1/2 -translate-y-1/2 p-3 rounded-full bg-black/50 hover:bg-black/80 text-white/70 hover:text-white transition z-10"
          title="Next (Right arrow)"
        >
          <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </button>
      )}
    </div>
  );
}
