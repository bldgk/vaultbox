import { useState, useRef, useCallback } from "react";
import { useFileStore, OpenTab } from "../store/fileStore";
import { getViewerType } from "../lib/fileTypes";
import { TextEditor } from "./viewers/TextEditor";
import { ImageViewer } from "./viewers/ImageViewer";
import { MediaViewer } from "./viewers/MediaViewer";
import { HexViewer } from "./viewers/HexViewer";
import { PdfViewer } from "./viewers/PdfViewer";
import { ArchiveViewer } from "./viewers/ArchiveViewer";

function renderViewer(tab: OpenTab, tabIndex: number) {
  const viewerType = getViewerType(tab.name);
  const effectiveType = tab.content?.type === "Text" ? "text" : viewerType;
  return (
    <>
      {effectiveType === "text" && <TextEditor tabIndex={tabIndex} />}
      {effectiveType === "image" && <ImageViewer tabIndex={tabIndex} />}
      {effectiveType === "media" && <MediaViewer tabIndex={tabIndex} />}
      {effectiveType === "archive" && <ArchiveViewer tabIndex={tabIndex} />}
      {effectiveType === "pdf" && <PdfViewer tabIndex={tabIndex} />}
      {effectiveType === "hex" && <HexViewer tabIndex={tabIndex} />}
    </>
  );
}

export function ViewerPanel() {
  const {
    openTabs,
    activeTabIndex,
    setActiveTab,
    closeTab,
    reorderTab,
    splitView,
    splitTabIndex,
    toggleSplitView,
    setSplitTab,
  } = useFileStore();

  // Pointer-based tab reorder (no HTML5 drag API — conflicts with Tauri)
  const [draggingIndex, setDraggingIndex] = useState<number | null>(null);
  const [dropTarget, setDropTarget] = useState<number | null>(null);
  const tabBarRef = useRef<HTMLDivElement>(null);
  const startX = useRef(0);
  const didMove = useRef(false);

  const handlePointerDown = useCallback((e: React.PointerEvent, index: number) => {
    // Only primary button, ignore close button clicks
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    startX.current = e.clientX;
    didMove.current = false;
    setDraggingIndex(index);
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }, []);

  const handlePointerMove = useCallback((e: React.PointerEvent) => {
    if (draggingIndex === null || !tabBarRef.current) return;
    if (!didMove.current && Math.abs(e.clientX - startX.current) < 5) return;
    didMove.current = true;

    // Find which tab we're hovering over
    const tabs = tabBarRef.current.children;
    for (let i = 0; i < tabs.length; i++) {
      const rect = (tabs[i] as HTMLElement).getBoundingClientRect();
      if (e.clientX >= rect.left && e.clientX < rect.right) {
        setDropTarget(i !== draggingIndex ? i : null);
        return;
      }
    }
    setDropTarget(null);
  }, [draggingIndex]);

  const handlePointerUp = useCallback(() => {
    if (draggingIndex !== null && dropTarget !== null && draggingIndex !== dropTarget) {
      reorderTab(draggingIndex, dropTarget);
    }
    setDraggingIndex(null);
    setDropTarget(null);
  }, [draggingIndex, dropTarget, reorderTab]);

  const handleTabClick = useCallback((e: React.MouseEvent, index: number) => {
    if (splitView && (e.altKey || e.metaKey)) {
      setSplitTab(index);
    } else {
      setActiveTab(index);
    }
  }, [splitView, setSplitTab, setActiveTab]);

  if (openTabs.length === 0) {
    return null;
  }

  const activeTab = openTabs[activeTabIndex];
  const splitTab = splitTabIndex >= 0 ? openTabs[splitTabIndex] : null;

  return (
    <div className="flex flex-col border-l border-gray-800 w-[50%] min-w-[300px]">
      {/* Tab bar */}
      <div className="flex bg-gray-900 border-b border-gray-800">
        <div ref={tabBarRef} className="flex overflow-x-auto flex-1">
          {openTabs.map((tab, i) => (
            <div
              key={tab.path}
              onPointerDown={(e) => handlePointerDown(e, i)}
              onPointerMove={handlePointerMove}
              onPointerUp={handlePointerUp}
              className={`flex items-center gap-1.5 px-3 py-1.5 text-sm cursor-pointer border-r border-gray-800 shrink-0 transition-all select-none ${
                i === activeTabIndex
                  ? "bg-gray-950 text-white"
                  : i === splitTabIndex && splitView
                    ? "bg-gray-950/70 text-indigo-300"
                    : "bg-gray-900 text-gray-500 hover:text-gray-300"
              } ${
                dropTarget === i ? "border-l-2 border-l-indigo-500 pl-2" : ""
              } ${
                draggingIndex === i && didMove.current ? "opacity-50" : ""
              }`}
              onClick={(e) => handleTabClick(e, i)}
            >
              <span className="truncate max-w-[120px]">
                {tab.modified && <span className="text-yellow-500 mr-1">*</span>}
                {tab.name}
              </span>
              <button
                onClick={(e) => { e.stopPropagation(); closeTab(i); }}
                className="p-0.5 rounded hover:bg-gray-700"
              >
                <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          ))}
        </div>
        {/* Split view toggle button */}
        <button
          onClick={toggleSplitView}
          className={`flex items-center justify-center px-2 shrink-0 border-l border-gray-800 transition-colors ${
            splitView
              ? "bg-indigo-600 text-white hover:bg-indigo-500"
              : "bg-gray-900 text-gray-500 hover:text-gray-300 hover:bg-gray-800"
          }`}
          title={splitView ? "Close split view" : "Split view"}
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <rect x="3" y="3" width="18" height="18" rx="2" />
            <line x1="12" y1="3" x2="12" y2="21" />
          </svg>
        </button>
      </div>

      {/* Viewer content */}
      {splitView ? (
        <div className="flex-1 flex flex-row overflow-hidden">
          {/* Left half — active tab */}
          <div className="flex-1 flex flex-col overflow-hidden min-w-0">
            <div className="px-2 py-0.5 text-xs text-gray-500 bg-gray-900/50 border-b border-gray-800 truncate">
              {activeTab ? activeTab.name : "No file"}
            </div>
            <div className="flex-1 overflow-hidden">
              {activeTab && renderViewer(activeTab, activeTabIndex)}
            </div>
          </div>
          {/* Vertical divider */}
          <div className="w-px bg-gray-700 shrink-0" />
          {/* Right half — split tab */}
          <div className="flex-1 flex flex-col overflow-hidden min-w-0">
            <div className="px-2 py-0.5 text-xs text-indigo-400 bg-gray-900/50 border-b border-gray-800 truncate">
              {splitTab ? splitTab.name : "Split"}
            </div>
            <div className="flex-1 overflow-hidden">
              {splitTab ? (
                renderViewer(splitTab, splitTabIndex)
              ) : (
                <div className="flex items-center justify-center h-full text-gray-600 text-sm">
                  <p>Alt+click a tab to open here</p>
                </div>
              )}
            </div>
          </div>
        </div>
      ) : (
        <div className="flex-1 overflow-hidden">
          {activeTab && renderViewer(activeTab, activeTabIndex)}
        </div>
      )}
    </div>
  );
}
