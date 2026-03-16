import { useEffect, useRef, useState, useCallback } from "react";
import { useFileStore } from "../../store/fileStore";
import * as pdfjsLib from "pdfjs-dist";
import type { PDFDocumentProxy } from "pdfjs-dist";

pdfjsLib.GlobalWorkerOptions.workerSrc = new URL(
  "pdfjs-dist/build/pdf.worker.min.mjs",
  import.meta.url,
).toString();

type ZoomPreset = "fit-width" | "fit-page" | number;

const ZOOM_LEVELS = [0.5, 0.75, 1, 1.25, 1.5, 2] as const;

function base64ToUint8Array(base64: string): Uint8Array {
  const raw = atob(base64);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

function getZoomLabel(zoom: ZoomPreset): string {
  if (zoom === "fit-width") return "Fit Width";
  if (zoom === "fit-page") return "Fit Page";
  return `${Math.round(zoom * 100)}%`;
}

function resolveScale(
  zoom: ZoomPreset,
  page: pdfjsLib.PDFPageProxy,
  containerWidth: number,
  containerHeight: number,
): number {
  const viewport = page.getViewport({ scale: 1 });
  if (zoom === "fit-width") {
    return (containerWidth - 48) / viewport.width;
  }
  if (zoom === "fit-page") {
    const scaleW = (containerWidth - 48) / viewport.width;
    const scaleH = (containerHeight - 48) / viewport.height;
    return Math.min(scaleW, scaleH);
  }
  return zoom;
}

export function PdfViewer({ tabIndex }: { tabIndex: number }) {
  const { openTabs } = useFileStore();
  const tab = openTabs[tabIndex];
  const content = tab?.content;

  const [pdfDoc, setPdfDoc] = useState<PDFDocumentProxy | null>(null);
  const [numPages, setNumPages] = useState(0);
  const [currentPage, setCurrentPage] = useState(1);
  const [zoom, setZoom] = useState<ZoomPreset>("fit-width");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRefs = useRef<Map<number, HTMLCanvasElement>>(new Map());
  const renderTasksRef = useRef<Map<number, pdfjsLib.RenderTask>>(new Map());
  const pageObserverRef = useRef<IntersectionObserver | null>(null);
  const pageElementsRef = useRef<Map<number, HTMLDivElement>>(new Map());

  // Load PDF document
  useEffect(() => {
    if (!content || content.type !== "Binary") return;

    setLoading(true);
    setError(null);

    const data = base64ToUint8Array(content.data);
    const loadingTask = pdfjsLib.getDocument({ data });

    let cancelled = false;

    loadingTask.promise
      .then((doc) => {
        if (cancelled) {
          doc.destroy();
          return;
        }
        setPdfDoc(doc);
        setNumPages(doc.numPages);
        setCurrentPage(1);
        setLoading(false);
      })
      .catch((err) => {
        if (!cancelled) {
          setError(`Failed to load PDF: ${err.message}`);
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [content]);

  // Render a single page onto its canvas
  const renderPage = useCallback(
    async (pageNum: number) => {
      if (!pdfDoc || !containerRef.current) return;

      const canvas = canvasRefs.current.get(pageNum);
      if (!canvas) return;

      // Cancel any existing render for this page
      const existingTask = renderTasksRef.current.get(pageNum);
      if (existingTask) {
        existingTask.cancel();
        renderTasksRef.current.delete(pageNum);
      }

      try {
        const page = await pdfDoc.getPage(pageNum);
        const containerRect = containerRef.current.getBoundingClientRect();
        const scale = resolveScale(
          zoom,
          page,
          containerRect.width,
          containerRect.height,
        );
        const dpr = window.devicePixelRatio || 1;
        const viewport = page.getViewport({ scale: scale * dpr });
        const displayViewport = page.getViewport({ scale });

        canvas.width = viewport.width;
        canvas.height = viewport.height;
        canvas.style.width = `${displayViewport.width}px`;
        canvas.style.height = `${displayViewport.height}px`;

        // Also size the parent wrapper so layout is correct before render
        const wrapper = pageElementsRef.current.get(pageNum);
        if (wrapper) {
          wrapper.style.width = `${displayViewport.width}px`;
          wrapper.style.height = `${displayViewport.height}px`;
        }

        const renderTask = page.render({
          canvas,
          viewport,
        });
        renderTasksRef.current.set(pageNum, renderTask);

        await renderTask.promise;
        renderTasksRef.current.delete(pageNum);
      } catch (err: unknown) {
        if (err instanceof Error && err.message !== "Rendering cancelled") {
          setError(`Error rendering page ${pageNum}: ${err.message}`);
        }
      }
    },
    [pdfDoc, zoom],
  );

  // Render all pages when doc or zoom changes
  useEffect(() => {
    if (!pdfDoc || !containerRef.current) return;

    const renderAll = async () => {
      for (let i = 1; i <= pdfDoc.numPages; i++) {
        await renderPage(i);
      }
    };

    // Small delay to ensure DOM layout is ready
    const timer = requestAnimationFrame(() => {
      renderAll();
    });

    return () => {
      cancelAnimationFrame(timer);
      renderTasksRef.current.forEach((task) => {
        try { task.cancel(); } catch {}
      });
      renderTasksRef.current.clear();
    };
  }, [pdfDoc, zoom, renderPage]);

  // Track which page is currently most visible via IntersectionObserver
  useEffect(() => {
    if (!pdfDoc || numPages === 0) return;

    const observer = new IntersectionObserver(
      (entries) => {
        let maxRatio = 0;
        let maxPage = currentPage;
        for (const entry of entries) {
          const pageNum = Number(
            (entry.target as HTMLElement).dataset.pageNum,
          );
          if (entry.intersectionRatio > maxRatio) {
            maxRatio = entry.intersectionRatio;
            maxPage = pageNum;
          }
        }
        if (maxRatio > 0) {
          setCurrentPage(maxPage);
        }
      },
      {
        root: containerRef.current,
        threshold: [0, 0.25, 0.5, 0.75, 1],
      },
    );

    pageObserverRef.current = observer;
    pageElementsRef.current.forEach((el) => observer.observe(el));

    return () => {
      observer.disconnect();
      pageObserverRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pdfDoc, numPages]);

  // Zoom step helpers
  const zoomIn = useCallback(() => {
    setZoom((prev) => {
      const current =
        typeof prev === "number"
          ? prev
          : containerRef.current && pdfDoc
            ? 1
            : 1;
      for (const level of ZOOM_LEVELS) {
        if (level > current + 0.01) return level;
      }
      return current;
    });
  }, [pdfDoc]);

  const zoomOut = useCallback(() => {
    setZoom((prev) => {
      const current =
        typeof prev === "number"
          ? prev
          : containerRef.current && pdfDoc
            ? 1
            : 1;
      for (let i = ZOOM_LEVELS.length - 1; i >= 0; i--) {
        if (ZOOM_LEVELS[i] < current - 0.01) return ZOOM_LEVELS[i];
      }
      return current;
    });
  }, [pdfDoc]);

  // Mouse wheel zoom (Ctrl/Cmd + scroll)
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handleWheel = (e: WheelEvent) => {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        if (e.deltaY < 0) {
          zoomIn();
        } else {
          zoomOut();
        }
      }
    };

    container.addEventListener("wheel", handleWheel, { passive: false });
    return () => container.removeEventListener("wheel", handleWheel);
  }, [zoomIn, zoomOut]);

  // Page navigation
  const goToPage = useCallback(
    (page: number) => {
      const el = pageElementsRef.current.get(page);
      if (el) {
        el.scrollIntoView({ behavior: "smooth", block: "start" });
      }
    },
    [],
  );

  const goPrevPage = useCallback(() => {
    if (currentPage > 1) goToPage(currentPage - 1);
  }, [currentPage, goToPage]);

  const goNextPage = useCallback(() => {
    if (currentPage < numPages) goToPage(currentPage + 1);
  }, [currentPage, numPages, goToPage]);

  // Register canvas ref
  const setCanvasRef = useCallback(
    (pageNum: number) => (el: HTMLCanvasElement | null) => {
      if (el) {
        canvasRefs.current.set(pageNum, el);
      } else {
        canvasRefs.current.delete(pageNum);
      }
    },
    [],
  );

  // Register page wrapper ref + observe
  const setPageRef = useCallback(
    (pageNum: number) => (el: HTMLDivElement | null) => {
      if (el) {
        pageElementsRef.current.set(pageNum, el);
        pageObserverRef.current?.observe(el);
      } else {
        const prev = pageElementsRef.current.get(pageNum);
        if (prev) pageObserverRef.current?.unobserve(prev);
        pageElementsRef.current.delete(pageNum);
      }
    },
    [],
  );

  if (!content) return null;

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center bg-gray-950 text-gray-400">
        Loading PDF...
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex-1 flex items-center justify-center bg-gray-950 text-red-400">
        {error}
      </div>
    );
  }

  const pages = Array.from({ length: numPages }, (_, i) => i + 1);

  return (
    <div className="flex-1 flex flex-col bg-gray-950 overflow-hidden">
      {/* Toolbar */}
      <div className="flex items-center gap-2 px-3 py-1.5 bg-gray-900 border-b border-gray-800 text-sm shrink-0">
        {/* Page navigation */}
        <button
          onClick={goPrevPage}
          disabled={currentPage <= 1}
          className="p-1 rounded hover:bg-gray-700 disabled:opacity-30 disabled:cursor-not-allowed text-gray-300"
          title="Previous page"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <span className="text-gray-400 min-w-[80px] text-center tabular-nums">
          Page {currentPage} of {numPages}
        </span>
        <button
          onClick={goNextPage}
          disabled={currentPage >= numPages}
          className="p-1 rounded hover:bg-gray-700 disabled:opacity-30 disabled:cursor-not-allowed text-gray-300"
          title="Next page"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </button>

        <div className="w-px h-5 bg-gray-700 mx-1" />

        {/* Zoom controls */}
        <button
          onClick={zoomOut}
          className="p-1 rounded hover:bg-gray-700 text-gray-300"
          title="Zoom out"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 12H4" />
          </svg>
        </button>
        <span className="text-gray-400 min-w-[70px] text-center text-xs">
          {getZoomLabel(zoom)}
        </span>
        <button
          onClick={zoomIn}
          className="p-1 rounded hover:bg-gray-700 text-gray-300"
          title="Zoom in"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
        </button>

        <div className="w-px h-5 bg-gray-700 mx-1" />

        <button
          onClick={() => setZoom("fit-width")}
          className={`px-2 py-0.5 rounded text-xs ${
            zoom === "fit-width"
              ? "bg-indigo-600 text-white"
              : "text-gray-400 hover:bg-gray-700 hover:text-gray-300"
          }`}
          title="Fit width"
        >
          Fit Width
        </button>
        <button
          onClick={() => setZoom("fit-page")}
          className={`px-2 py-0.5 rounded text-xs ${
            zoom === "fit-page"
              ? "bg-indigo-600 text-white"
              : "text-gray-400 hover:bg-gray-700 hover:text-gray-300"
          }`}
          title="Fit page"
        >
          Fit Page
        </button>
      </div>

      {/* Scrollable page container */}
      <div
        ref={containerRef}
        className="flex-1 overflow-auto"
      >
        <div className="flex flex-col items-center py-4">
          {pages.map((pageNum) => (
            <div
              key={pageNum}
              ref={setPageRef(pageNum)}
              data-page-num={pageNum}
              className="mb-4 shadow-lg border border-gray-800 bg-white"
              style={{ lineHeight: 0 }}
            >
              <canvas ref={setCanvasRef(pageNum)} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
