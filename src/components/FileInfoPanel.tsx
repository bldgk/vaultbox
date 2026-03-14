import { useFileStore } from "../store/fileStore";
import { formatFileSize, formatDate, getViewerType } from "../lib/fileTypes";
import type { ViewerType } from "../lib/fileTypes";

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif"]);

function viewerTypeLabel(vt: ViewerType): string {
  switch (vt) {
    case "text": return "Text";
    case "image": return "Image";
    case "media": return "Media";
    case "pdf": return "PDF";
    case "hex": return "Binary";
    case "archive": return "Archive";
  }
}

export function FileInfoPanel() {
  const { showInfoPanel, toggleInfoPanel, selectedFiles, entries, currentPath } = useFileStore();

  if (!showInfoPanel) return null;

  // Only show when exactly one file is selected
  if (selectedFiles.size !== 1) {
    return (
      <div className="w-64 border-l border-gray-800 bg-gray-900 flex flex-col shrink-0">
        <div className="flex items-center justify-between px-3 py-2 border-b border-gray-800">
          <span className="text-xs font-medium text-gray-400 uppercase tracking-wide">File Info</span>
          <button
            onClick={toggleInfoPanel}
            className="text-gray-500 hover:text-white p-0.5 rounded hover:bg-gray-800"
            title="Close (Cmd+I)"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="flex-1 flex items-center justify-center px-4">
          <p className="text-xs text-gray-500 text-center">Select a single file to view its details</p>
        </div>
      </div>
    );
  }

  const selectedName = Array.from(selectedFiles)[0];
  const entry = entries.find((e) => e.name === selectedName);
  if (!entry) return null;

  const fullPath = currentPath ? `${currentPath}/${entry.name}` : entry.name;
  const ext = entry.name.split(".").pop()?.toLowerCase() ?? "";
  const viewerType = entry.is_dir ? "folder" : getViewerType(entry.name);
  const isImage = IMAGE_EXTS.has(ext) && !entry.is_dir;

  return (
    <div className="w-64 border-l border-gray-800 bg-gray-900 flex flex-col shrink-0 overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-gray-800">
        <span className="text-xs font-medium text-gray-400 uppercase tracking-wide">File Info</span>
        <button
          onClick={toggleInfoPanel}
          className="text-gray-500 hover:text-white p-0.5 rounded hover:bg-gray-800"
          title="Close (Cmd+I)"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-3 py-3 space-y-4">
        {/* File icon and name */}
        <div className="flex flex-col items-center gap-2 pb-3 border-b border-gray-800">
          <FileTypeIcon isDir={entry.is_dir} name={entry.name} />
          <h3 className="text-sm font-bold text-gray-100 text-center break-all leading-tight">
            {entry.name}
          </h3>
        </div>

        {/* Image preview */}
        {isImage && (
          <div className="border border-gray-800 rounded-lg overflow-hidden">
            <img
              src={`vaultmedia://localhost/${encodeURIComponent(fullPath)}`}
              alt={entry.name}
              className="w-full h-auto max-h-40 object-contain bg-gray-950"
              loading="lazy"
            />
          </div>
        )}

        {/* Details */}
        <div className="space-y-2.5">
          <InfoRow label="Type" value={entry.is_dir ? "Folder" : `${viewerTypeLabel(viewerType as ViewerType)} (.${ext})`} />
          {!entry.is_dir && (
            <InfoRow label="Size" value={formatFileSize(entry.size)} />
          )}
          <InfoRow label="Modified" value={formatDate(entry.modified)} />
          <InfoRow label="Path" value={fullPath} mono />
          <InfoRow label="Encrypted" value={entry.encrypted_name} mono />
        </div>
      </div>
    </div>
  );
}

function InfoRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  if (!value) return null;
  return (
    <div>
      <dt className="text-[10px] font-medium text-gray-500 uppercase tracking-wide mb-0.5">{label}</dt>
      <dd className={`text-xs text-gray-300 break-all ${mono ? "font-mono text-[11px]" : ""}`}>{value}</dd>
    </div>
  );
}

function FileTypeIcon({ isDir, name }: { isDir: boolean; name: string }) {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  const s = "w-10 h-10";

  if (isDir) {
    return (
      <svg className={`${s} text-indigo-400`} fill="currentColor" viewBox="0 0 24 24">
        <path d="M10 4H4a2 2 0 00-2 2v12a2 2 0 002 2h16a2 2 0 002-2V8a2 2 0 00-2-2h-8l-2-2z" />
      </svg>
    );
  }

  if (IMAGE_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-emerald-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
      </svg>
    );
  }

  const VIDEO_EXTS = new Set(["mp4", "m4v", "mov", "webm", "ogg", "ogv", "avi", "mkv", "3gp", "ts"]);
  if (VIDEO_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-purple-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" />
      </svg>
    );
  }

  const AUDIO_EXTS = new Set(["mp3", "wav", "m4a", "flac", "aac"]);
  if (AUDIO_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-amber-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
      </svg>
    );
  }

  if (ext === "pdf") {
    return (
      <svg className={`${s} text-red-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 13h6m-6 3h4" />
      </svg>
    );
  }

  // Text / code files
  const TEXT_EXTS = new Set([
    "txt", "md", "json", "yml", "yaml", "csv", "log", "xml", "html", "css",
    "js", "ts", "jsx", "tsx", "py", "rs", "go", "java", "c", "cpp", "h",
    "hpp", "sh", "bash", "zsh", "toml", "ini", "cfg", "conf", "sql",
  ]);
  if (TEXT_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-sky-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 13h6m-6 3h6m-6-6h3" />
      </svg>
    );
  }

  // Generic file
  return (
    <svg className={`${s} text-gray-500`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
    </svg>
  );
}
