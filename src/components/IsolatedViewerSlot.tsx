import { useEffect, useRef, useCallback } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import {
  createViewerWebview,
  closeViewerWebview,
  resizeViewerWebview,
} from "../hooks/useTauriCommands";
import { useFileStore, type OpenTab } from "../store/fileStore";

interface Props {
  tab: OpenTab;
  tabIndex: number;
  /** Unique label for this viewer webview */
  label: string;
}

/**
 * Renders a placeholder div and manages an isolated Tauri child webview
 * positioned over it. The child webview has its own V8 heap — when it's
 * destroyed (on tab close), all plaintext strings are reclaimed.
 */
export function IsolatedViewerSlot({ tab, tabIndex, label }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const webviewCreated = useRef(false);
  const lastBounds = useRef({ x: 0, y: 0, w: 0, h: 0 });

  const updateBounds = useCallback(async () => {
    if (!containerRef.current || !webviewCreated.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    const x = rect.left;
    const y = rect.top;
    const w = rect.width;
    const h = rect.height;

    // Skip if bounds haven't changed
    const last = lastBounds.current;
    if (
      Math.abs(last.x - x) < 1 &&
      Math.abs(last.y - y) < 1 &&
      Math.abs(last.w - w) < 1 &&
      Math.abs(last.h - h) < 1
    ) {
      return;
    }

    lastBounds.current = { x, y, w, h };

    try {
      await resizeViewerWebview(label, x, y, w, h);
    } catch {
      // Webview may have been destroyed already
    }
  }, [label]);

  useEffect(() => {
    if (!containerRef.current) return;

    const rect = containerRef.current.getBoundingClientRect();
    const x = rect.left;
    const y = rect.top;
    const w = rect.width;
    const h = rect.height;
    lastBounds.current = { x, y, w, h };

    let destroyed = false;

    // Listen for ready signal from the viewer webview
    const unlistenReady = listen(`viewer:ready:${label}`, () => {
      if (destroyed) return;
      // Send the file data to the viewer webview
      emit(`viewer:load:${label}`, {
        path: tab.path,
        name: tab.name,
      });
    });

    // Listen for modified state changes from the viewer
    const unlistenModified = listen<{ modified: boolean }>(
      `viewer:modified:${label}`,
      (event) => {
        if (destroyed) return;
        useFileStore.getState().markTabModified(tabIndex, event.payload.modified);
      }
    );

    // Listen for save completions from the viewer
    const unlistenSaved = listen<{ content: { type: string; data: string } }>(
      `viewer:saved:${label}`,
      (event) => {
        if (destroyed) return;
        const content = event.payload.content;
        if (content.type === "Text") {
          useFileStore
            .getState()
            .updateTabContent(tabIndex, { type: "Text", data: content.data });
        }
      }
    );

    // Create the child webview
    createViewerWebview(label, x, y, w, h)
      .then(() => {
        if (!destroyed) {
          webviewCreated.current = true;
        }
      })
      .catch((err) => {
        console.error(`Failed to create viewer webview '${label}':`, err);
      });

    // Observe container resize to reposition the webview
    const resizeObserver = new ResizeObserver(() => {
      updateBounds();
    });
    resizeObserver.observe(containerRef.current);

    // Also reposition on window resize/scroll
    window.addEventListener("resize", updateBounds);

    return () => {
      destroyed = true;
      webviewCreated.current = false;
      resizeObserver.disconnect();
      window.removeEventListener("resize", updateBounds);
      unlistenReady.then((fn) => fn());
      unlistenModified.then((fn) => fn());
      unlistenSaved.then((fn) => fn());

      // Destroy the child webview — this reclaims its entire V8 heap
      closeViewerWebview(label).catch(() => {});
    };
  }, [label, tab.path, tab.name, tabIndex, updateBounds]);

  return (
    <div
      ref={containerRef}
      className="flex-1 w-full h-full"
      style={{ minHeight: 0 }}
    />
  );
}
