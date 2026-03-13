import { useMemo } from "react";
import { useFileStore } from "../../store/fileStore";

export function ImageViewer({ tabIndex }: { tabIndex: number }) {
  const { openTabs, setFullscreenPreview } = useFileStore();
  const tab = openTabs[tabIndex];
  const content = tab?.content;

  const src = useMemo(() => {
    if (!content) return null;
    if (content.type === "Binary") {
      const ext = tab.name.split(".").pop()?.toLowerCase() ?? "png";
      const mime = ext === "svg" ? "image/svg+xml" : `image/${ext === "jpg" ? "jpeg" : ext}`;
      return `data:${mime};base64,${content.data}`;
    }
    if (content.type === "Text" && tab.name.endsWith(".svg")) {
      return `data:image/svg+xml;base64,${btoa(content.data)}`;
    }
    return null;
  }, [content, tab]);

  if (!src) return <div className="flex-1 flex items-center justify-center text-gray-500">Cannot display image</div>;

  const handleFullscreen = () => {
    if (tab && content) {
      setFullscreenPreview({ filePath: tab.path, fileName: tab.name, content });
    }
  };

  return (
    <div className="flex-1 flex items-center justify-center bg-gray-950 overflow-auto p-4 relative group">
      <img
        src={src}
        alt={tab.name}
        className="max-w-full max-h-full object-contain cursor-pointer"
        onDoubleClick={handleFullscreen}
      />
      <button
        onClick={handleFullscreen}
        className="absolute top-3 right-3 p-1.5 bg-black/60 rounded-lg opacity-0 group-hover:opacity-100 transition text-white hover:bg-black/80"
        title="Fullscreen (double-click image)"
      >
        <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
        </svg>
      </button>
    </div>
  );
}
