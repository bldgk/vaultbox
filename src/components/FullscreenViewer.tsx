import { useEffect, useMemo, useCallback, useState, useRef } from "react";
import { useFileStore } from "../store/fileStore";
import { useDialogStore } from "../store/dialogStore";
import { deleteEntry } from "../hooks/useTauriCommands";
import { getViewerType } from "../lib/fileTypes";

const PREVIEWABLE_TYPES = new Set(["image", "media"]);

const VIDEO_EXTENSIONS = new Set([
  "mp4", "m4v", "mov", "ogv", "webm", "avi", "mkv", "3gp",
]);

function getMediaUrl(filePath: string): string {
  return `vaultmedia://localhost/${encodeURIComponent(filePath)}`;
}

function isVideoFile(filename: string): boolean {
  const ext = filename.split(".").pop()?.toLowerCase() ?? "";
  return VIDEO_EXTENSIONS.has(ext);
}

export function FullscreenViewer() {
  const { fullscreenPreview, setFullscreenPreview, entries, currentPath } = useFileStore();
  const { showConfirm } = useDialogStore();
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [rotation, setRotation] = useState(0);
  const [isDragging, setIsDragging] = useState(false);
  const [videoTime, setVideoTime] = useState(0);
  const [videoDuration, setVideoDuration] = useState(0);
  const [videoPaused, setVideoPaused] = useState(false);
  const dragStart = useRef({ x: 0, y: 0, panX: 0, panY: 0 });
  const videoRef = useRef<HTMLVideoElement>(null);
  const thumbnailStripRef = useRef<HTMLDivElement>(null);
  const activeThumbnailRef = useRef<HTMLButtonElement>(null);

  // Get list of previewable files in current directory
  const previewableFiles = useMemo(() => {
    return entries
      .filter((e) => !e.is_dir && PREVIEWABLE_TYPES.has(getViewerType(e.name)))
      .map((e) => ({
        name: e.name,
        path: currentPath ? `${currentPath}/${e.name}` : e.name,
        isVideo: isVideoFile(e.name),
        viewerType: getViewerType(e.name),
      }));
  }, [entries, currentPath]);

  const currentIndex = useMemo(() => {
    if (!fullscreenPreview) return -1;
    return previewableFiles.findIndex((f) => f.path === fullscreenPreview.filePath);
  }, [fullscreenPreview, previewableFiles]);

  const showThumbnailStrip = previewableFiles.length >= 2;

  const navigateToFile = useCallback((index: number) => {
    if (index < 0 || index >= previewableFiles.length) return;
    const file = previewableFiles[index];
    setZoom(1);
    setPan({ x: 0, y: 0 });
    setRotation(0);
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

  const goFirst = useCallback(() => {
    if (previewableFiles.length > 0) navigateToFile(0);
  }, [previewableFiles.length, navigateToFile]);

  const goLast = useCallback(() => {
    if (previewableFiles.length > 0) navigateToFile(previewableFiles.length - 1);
  }, [previewableFiles.length, navigateToFile]);

  const handleRotate = useCallback(() => {
    setRotation((r) => (r + 90) % 360);
  }, []);

  const handleDelete = useCallback(() => {
    if (!fullscreenPreview) return;
    const fileName = fullscreenPreview.fileName;
    const filePath = fullscreenPreview.filePath;
    showConfirm({
      title: "Delete File",
      message: `Delete "${fileName}" permanently? This cannot be undone.`,
      confirmLabel: "Delete",
      danger: true,
      onConfirm: async () => {
        try {
          await deleteEntry(filePath, true);
          // Close tabs for deleted file
          const state = useFileStore.getState();
          const toClose = state.openTabs
            .map((t, i) => ({ path: t.path, index: i }))
            .filter((t) => t.path === filePath)
            .reverse();
          for (const t of toClose) {
            useFileStore.getState().closeTab(t.index);
          }
          // Navigate to next/prev or close fullscreen
          if (previewableFiles.length <= 1) {
            setFullscreenPreview(null);
          } else if (currentIndex < previewableFiles.length - 1) {
            // Will navigate to next (which shifts into current index after refresh)
            const nextFile = previewableFiles[currentIndex + 1];
            setFullscreenPreview({
              filePath: nextFile.path,
              fileName: nextFile.name,
              content: { type: "Binary", data: "" },
            });
          } else {
            const prevFile = previewableFiles[currentIndex - 1];
            setFullscreenPreview({
              filePath: prevFile.path,
              fileName: prevFile.name,
              content: { type: "Binary", data: "" },
            });
          }
          setRotation(0);
          useFileStore.getState().refresh();
        } catch (err) {
          showConfirm({
            title: "Error",
            message: `Failed to delete: ${err}`,
            confirmLabel: "OK",
            onConfirm: () => {},
          });
        }
      },
    });
  }, [fullscreenPreview, previewableFiles, currentIndex, setFullscreenPreview, showConfirm]);

  // Derive viewer type and sources (must be before early return so hooks are stable)
  const viewerType = fullscreenPreview ? getViewerType(fullscreenPreview.fileName) : null;
  const isImage = viewerType === "image";
  const isVideo = viewerType === "media";

  // Both images and videos use vaultmedia:// — no base64, no blob URLs, instant
  const mediaSrc = useMemo(() => {
    if (!fullscreenPreview || (!isImage && !isVideo)) return null;
    return getMediaUrl(fullscreenPreview.filePath);
  }, [fullscreenPreview, isImage, isVideo]);

  // Auto-scroll thumbnail strip to keep active thumbnail visible
  useEffect(() => {
    if (activeThumbnailRef.current && thumbnailStripRef.current) {
      activeThumbnailRef.current.scrollIntoView({
        behavior: "smooth",
        block: "nearest",
        inline: "center",
      });
    }
  }, [currentIndex]);

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
      } else if (e.key === "Home") {
        e.preventDefault();
        goFirst();
      } else if (e.key === "End") {
        e.preventDefault();
        goLast();
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
      } else if (e.key === "r" || e.key === "R") {
        handleRotate();
      } else if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        handleDelete();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [fullscreenPreview, goNext, goPrev, goFirst, goLast, setFullscreenPreview, handleRotate, handleDelete]);

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
    <div className="fixed inset-0 z-[100] bg-black flex flex-col select-none" role="dialog" aria-modal="true" aria-label={`Fullscreen preview: ${fullscreenPreview.fileName}`}>
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
              <button onClick={() => setZoom((z) => Math.max(z / 1.3, 0.1))} className="p-1.5 rounded hover:bg-white/10 text-gray-300" title="Zoom out (-)">
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 12H4" /></svg>
              </button>
              <span className="text-xs text-gray-400 w-12 text-center">{Math.round(zoom * 100)}%</span>
              <button onClick={() => setZoom((z) => Math.min(z * 1.3, 10))} className="p-1.5 rounded hover:bg-white/10 text-gray-300" title="Zoom in (+)">
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" /></svg>
              </button>
              <button onClick={() => { setZoom(1); setPan({ x: 0, y: 0 }); }} className="p-1.5 rounded hover:bg-white/10 text-gray-300 text-xs" title="Reset zoom (0)">
                Fit
              </button>
              <div className="w-px h-5 bg-gray-700 mx-1" />
            </>
          )}
          <button onClick={handleRotate} className="p-1.5 rounded hover:bg-white/10 text-gray-300" title="Rotate (R)">
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </button>
          <button onClick={handleDelete} className="p-1.5 rounded hover:bg-white/10 text-red-400 hover:text-red-300" title="Delete (Del)">
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>
          <div className="w-px h-5 bg-gray-700 mx-1" />
          <button
            onClick={() => setFullscreenPreview(null)}
            className="p-1.5 rounded hover:bg-white/10 text-gray-300"
            title="Close (Esc)"
            aria-label="Close preview"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" aria-hidden="true">
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
              transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom}) rotate(${rotation}deg)`,
              maxWidth: zoom <= 1 && rotation % 180 === 0 ? "100%" : zoom <= 1 ? "100vh" : "none",
              maxHeight: zoom <= 1 && rotation % 180 === 0 ? "100%" : zoom <= 1 ? "100vw" : "none",
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
          <div className="relative flex items-center justify-center w-full h-full">
            <video
              ref={videoRef}
              controls={rotation === 0}
              autoPlay
              src={mediaSrc}
              className="w-full h-full object-contain transition-transform duration-200"
              style={{
                transform: rotation ? `rotate(${rotation}deg)` : undefined,
                ...(rotation % 180 !== 0 ? { maxWidth: "100vh", maxHeight: "100vw" } : {}),
              }}
              onTimeUpdate={() => { if (videoRef.current) setVideoTime(videoRef.current.currentTime); }}
              onLoadedMetadata={() => { if (videoRef.current) setVideoDuration(videoRef.current.duration); }}
              onPlay={() => setVideoPaused(false)}
              onPause={() => setVideoPaused(true)}
              onClick={() => { if (rotation && videoRef.current) { videoRef.current.paused ? videoRef.current.play() : videoRef.current.pause(); } }}
            />
            {rotation !== 0 && (
              <VideoControls videoRef={videoRef} time={videoTime} duration={videoDuration} paused={videoPaused} />
            )}
          </div>
        )}
        {!mediaSrc && (
          <div className="text-gray-500">Cannot display preview</div>
        )}
      </div>

      {/* Thumbnail strip */}
      {showThumbnailStrip && (
        <div className="bg-black/70 backdrop-blur-sm px-4 py-2 z-10">
          <div
            ref={thumbnailStripRef}
            className="flex gap-2 overflow-x-auto items-center justify-center scrollbar-thin scrollbar-thumb-gray-600 scrollbar-track-transparent"
            style={{ scrollbarWidth: "thin" }}
          >
            {previewableFiles.map((file, index) => {
              const isActive = index === currentIndex;
              return (
                <button
                  key={file.path}
                  ref={isActive ? activeThumbnailRef : undefined}
                  onClick={() => navigateToFile(index)}
                  className={`relative flex-shrink-0 w-[60px] h-[60px] rounded overflow-hidden transition-all ${
                    isActive
                      ? "ring-2 ring-indigo-500 ring-offset-1 ring-offset-black"
                      : "opacity-60 hover:opacity-100"
                  }`}
                  title={file.name}
                >
                  {file.isVideo ? (
                    /* Video thumbnail: dark placeholder with play icon overlay */
                    <div className="w-full h-full bg-gray-800 flex items-center justify-center">
                      <svg
                        className="w-6 h-6 text-white/80"
                        fill="currentColor"
                        viewBox="0 0 24 24"
                      >
                        <path d="M8 5v14l11-7z" />
                      </svg>
                    </div>
                  ) : file.viewerType === "image" ? (
                    /* Image thumbnail */
                    <img
                      src={getMediaUrl(file.path)}
                      alt={file.name}
                      className="w-full h-full object-cover"
                      draggable={false}
                      loading="lazy"
                    />
                  ) : (
                    /* Audio or other media: generic icon */
                    <div className="w-full h-full bg-gray-800 flex items-center justify-center">
                      <svg
                        className="w-5 h-5 text-white/60"
                        fill="none"
                        viewBox="0 0 24 24"
                        stroke="currentColor"
                      >
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth={2}
                          d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3"
                        />
                      </svg>
                    </div>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* Navigation arrows */}
      {hasPrev && (
        <button
          onClick={goPrev}
          className="absolute left-4 top-1/2 -translate-y-1/2 p-3 rounded-full bg-black/50 hover:bg-black/80 text-white/70 hover:text-white transition z-10"
          title="Previous (Left arrow)"
          aria-label="Previous file"
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
          aria-label="Next file"
        >
          <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </button>
      )}
    </div>
  );
}

function formatTime(s: number): string {
  if (!isFinite(s)) return "0:00";
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${m}:${sec.toString().padStart(2, "0")}`;
}

function VideoControls({ videoRef, time, duration, paused }: {
  videoRef: React.RefObject<HTMLVideoElement | null>;
  time: number;
  duration: number;
  paused: boolean;
}) {
  return (
    <div className="absolute bottom-4 left-1/2 -translate-x-1/2 flex items-center gap-3 px-4 py-2 bg-black/80 backdrop-blur-sm rounded-full z-20 min-w-[280px]">
      <button
        onClick={() => { if (videoRef.current) paused ? videoRef.current.play() : videoRef.current.pause(); }}
        className="text-white p-1"
      >
        {paused ? (
          <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24"><path d="M8 5v14l11-7z" /></svg>
        ) : (
          <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24"><path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" /></svg>
        )}
      </button>
      <span className="text-xs text-gray-400 w-10 text-right tabular-nums">{formatTime(time)}</span>
      <input
        type="range"
        min={0}
        max={duration || 1}
        step={0.1}
        value={time}
        onChange={(e) => { if (videoRef.current) videoRef.current.currentTime = Number(e.target.value); }}
        className="flex-1 h-1 accent-white cursor-pointer"
      />
      <span className="text-xs text-gray-400 w-10 tabular-nums">{formatTime(duration)}</span>
    </div>
  );
}
