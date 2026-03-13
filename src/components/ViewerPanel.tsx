import { useFileStore } from "../store/fileStore";
import { getViewerType } from "../lib/fileTypes";
import { TextEditor } from "./viewers/TextEditor";
import { ImageViewer } from "./viewers/ImageViewer";
import { MediaViewer } from "./viewers/MediaViewer";
import { HexViewer } from "./viewers/HexViewer";

export function ViewerPanel() {
  const { openTabs, activeTabIndex, setActiveTab, closeTab } = useFileStore();

  if (openTabs.length === 0) {
    return null;
  }

  const activeTab = openTabs[activeTabIndex];
  const viewerType = activeTab ? getViewerType(activeTab.name) : null;
  // If it's text content, use text viewer regardless of extension
  const effectiveType = activeTab?.content?.type === "Text" ? "text" : viewerType;

  return (
    <div className="flex flex-col border-l border-gray-800 w-[50%] min-w-[300px]">
      {/* Tab bar */}
      <div className="flex bg-gray-900 border-b border-gray-800 overflow-x-auto">
        {openTabs.map((tab, i) => (
          <div
            key={tab.path}
            className={`flex items-center gap-1.5 px-3 py-1.5 text-sm cursor-pointer border-r border-gray-800 shrink-0 ${
              i === activeTabIndex
                ? "bg-gray-950 text-white"
                : "bg-gray-900 text-gray-500 hover:text-gray-300"
            }`}
            onClick={() => setActiveTab(i)}
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

      {/* Viewer content */}
      <div className="flex-1 overflow-hidden">
        {activeTab && effectiveType === "text" && <TextEditor tabIndex={activeTabIndex} />}
        {activeTab && effectiveType === "image" && <ImageViewer tabIndex={activeTabIndex} />}
        {activeTab && effectiveType === "media" && <MediaViewer tabIndex={activeTabIndex} />}
        {activeTab && (effectiveType === "hex" || effectiveType === "pdf") && (
          <HexViewer tabIndex={activeTabIndex} />
        )}
      </div>
    </div>
  );
}
