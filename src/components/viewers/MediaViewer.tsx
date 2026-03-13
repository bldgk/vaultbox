import { useRef, useEffect } from "react";
import { useFileStore } from "../../store/fileStore";

const AUDIO_EXTENSIONS = new Set(["mp3", "wav", "ogg", "m4a", "flac", "aac"]);

function getMediaUrl(filePath: string): string {
  return `vaultmedia://localhost/${encodeURIComponent(filePath)}`;
}

export function MediaViewer({ tabIndex }: { tabIndex: number }) {
  const { openTabs, setFullscreenPreview, fullscreenPreview } = useFileStore();
  const tab = openTabs[tabIndex];
  const content = tab?.content;
  const videoRef = useRef<HTMLVideoElement>(null);
  const audioRef = useRef<HTMLAudioElement>(null);

  const ext = tab?.name.split(".").pop()?.toLowerCase() ?? "";
  const isAudio = AUDIO_EXTENSIONS.has(ext);

  const src = getMediaUrl(tab.path);

  // Space bar to toggle play/pause (skip when fullscreen viewer is active — it has its own handler)
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key !== " ") return;
      if (fullscreenPreview) return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;

      const el = isAudio ? audioRef.current : videoRef.current;
      if (!el) return;
      e.preventDefault();
      if (el.paused) el.play();
      else el.pause();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isAudio, fullscreenPreview]);

  const handleFullscreen = () => {
    if (tab && content) {
      setFullscreenPreview({ filePath: tab.path, fileName: tab.name, content });
    }
  };

  if (isAudio) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center bg-gray-950 gap-4 p-8">
        <svg className="w-16 h-16 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1} d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
        </svg>
        <span className="text-gray-400 text-sm">{tab.name}</span>
        <audio ref={audioRef} controls src={src} className="w-full max-w-md" />
        <span className="text-gray-600 text-xs">Space to play/pause</span>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col items-center justify-center bg-gray-950 p-4 gap-2">
      <div className="relative group">
        <video
          ref={videoRef}
          controls
          src={src}
          className="max-w-full max-h-[calc(100vh-200px)] rounded"
        />
        <button
          onClick={handleFullscreen}
          className="absolute top-2 right-2 p-1.5 bg-black/60 rounded-lg opacity-0 group-hover:opacity-100 transition text-white hover:bg-black/80"
          title="Fullscreen"
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
          </svg>
        </button>
      </div>
    </div>
  );
}
