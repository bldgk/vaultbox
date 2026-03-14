import { useEffect, useCallback, useState, useRef } from "react";
import { useFileStore } from "../store/fileStore";
import { listDir, readFile, deleteEntry, renameEntry, exportFile, copyEntry } from "../hooks/useTauriCommands";
import type { FileEntry } from "../hooks/useTauriCommands";
import { formatFileSize, formatDate, getViewerType } from "../lib/fileTypes";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import { save } from "@tauri-apps/plugin-dialog";

export function FileList() {
  const {
    currentPath, refreshCounter, entries, setEntries, selectedFiles, toggleSelection,
    navigateTo, openTab, viewMode, sortBy, sortAsc, setSortBy,
    searchResults, loading, setLoading, clipboard, setClipboard, setFullscreenPreview,
    startBusy, stopBusy,
  } = useFileStore();

  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; entry: FileEntry } | null>(null);
  const [renamingEntry, setRenamingEntry] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const renameInputRef = useRef<HTMLInputElement>(null);

  const loadEntries = useCallback(async () => {
    setLoading(true);
    useFileStore.getState().setStatusText("Loading directory...");
    try {
      const items = await listDir(currentPath);
      setEntries(items);
    } catch {
      // Failed to list directory
    } finally {
      setLoading(false);
      useFileStore.getState().setStatusText("");
    }
  }, [currentPath, refreshCounter, setEntries, setLoading]);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  // Focus rename input when it appears
  useEffect(() => {
    if (renamingEntry && renameInputRef.current) {
      renameInputRef.current.focus();
      // Select filename without extension
      const dotIdx = renameValue.lastIndexOf(".");
      if (dotIdx > 0) {
        renameInputRef.current.setSelectionRange(0, dotIdx);
      } else {
        renameInputRef.current.select();
      }
    }
  }, [renamingEntry]);

  const displayEntries = searchResults ?? entries;

  const sorted = [...displayEntries].sort((a, b) => {
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    let cmp = 0;
    switch (sortBy) {
      case "name": cmp = a.name.localeCompare(b.name); break;
      case "size": cmp = a.size - b.size; break;
      case "modified": cmp = a.modified - b.modified; break;
    }
    return sortAsc ? cmp : -cmp;
  });

  const fullPath = (name: string) => currentPath ? `${currentPath}/${name}` : name;

  const handleOpen = async (entry: FileEntry) => {
    if (entry.is_dir) {
      navigateTo(fullPath(entry.name));
    } else {
      const sizeStr = entry.size > 1024 * 1024
        ? `${(entry.size / 1024 / 1024).toFixed(1)} MB`
        : entry.size > 1024
        ? `${(entry.size / 1024).toFixed(0)} KB`
        : `${entry.size} B`;
      startBusy(`Opening ${entry.name} (${sizeStr})...`);
      try {
        const content = await readFile(fullPath(entry.name));
        openTab({ path: fullPath(entry.name), name: entry.name, content, modified: false });
      } catch (err) {
        alert(`Failed to read file: ${err}`);
      } finally {
        stopBusy();
      }
    }
  };

  const handleDelete = async (entry: FileEntry) => {
    if (confirm(`Delete "${entry.name}" permanently?`)) {
      try {
        await deleteEntry(fullPath(entry.name), true);
        useFileStore.getState().refresh();
      } catch (err) {
        alert(`Failed to delete: ${err}`);
      }
    }
  };

  const handleStartRename = (entry: FileEntry) => {
    setRenamingEntry(entry.name);
    setRenameValue(entry.name);
  };

  const handleRenameSubmit = async () => {
    if (!renamingEntry || !renameValue.trim() || renameValue === renamingEntry) {
      setRenamingEntry(null);
      return;
    }
    try {
      await renameEntry(fullPath(renamingEntry), renameValue.trim());
      setRenamingEntry(null);
      useFileStore.getState().refresh();
    } catch (err) {
      alert(`Failed to rename: ${err}`);
    }
  };

  const handleExport = async (entry: FileEntry) => {
    try {
      const dest = await save({ defaultPath: entry.name });
      if (dest) {
        await exportFile(fullPath(entry.name), dest);
      }
    } catch (err) {
      alert(`Failed to export: ${err}`);
    }
  };

  const handleContextMenu = (e: React.MouseEvent, entry: FileEntry) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY, entry });
  };

  const handleCopy = (entry: FileEntry) => {
    const path = fullPath(entry.name);
    setClipboard({
      files: [path],
      names: [entry.name],
      sourceDir: currentPath,
      operation: "copy",
    });
  };

  const handlePaste = async () => {
    if (!clipboard) return;
    startBusy(`Pasting ${clipboard.names.length} item(s)...`);
    try {
      for (let i = 0; i < clipboard.files.length; i++) {
        const sourcePath = clipboard.files[i];
        let destName = clipboard.names[i];
        if (clipboard.sourceDir === currentPath && clipboard.operation === "copy") {
          const dotIdx = destName.lastIndexOf(".");
          if (dotIdx > 0) {
            destName = destName.slice(0, dotIdx) + " copy" + destName.slice(dotIdx);
          } else {
            destName = destName + " copy";
          }
        }
        await copyEntry(sourcePath, currentPath, destName);
        if (clipboard.operation === "cut") {
          await deleteEntry(sourcePath, true);
        }
      }
      if (clipboard.operation === "cut") setClipboard(null);
      useFileStore.getState().refresh();
    } catch (err) {
      alert(`Paste failed: ${err}`);
    } finally {
      stopBusy();
    }
  };

  const handlePreview = async (entry: FileEntry) => {
    try {
      const content = await readFile(fullPath(entry.name));
      setFullscreenPreview({ filePath: fullPath(entry.name), fileName: entry.name, content });
    } catch (err) {
      alert(`Failed to preview: ${err}`);
    }
  };

  const getContextMenuItems = (entry: FileEntry): ContextMenuItem[] => {
    const vType = getViewerType(entry.name);
    const items: ContextMenuItem[] = [
      { label: entry.is_dir ? "Open" : "Open File", onClick: () => handleOpen(entry) },
    ];
    if (!entry.is_dir && (vType === "image" || vType === "media")) {
      items.push({ label: "Preview Fullscreen", onClick: () => handlePreview(entry) });
    }
    if (!entry.is_dir) {
      items.push({ label: "Export to disk...", onClick: () => handleExport(entry) });
    }
    items.push(
      { label: "Copy", onClick: () => handleCopy(entry), divider: true },
    );
    if (clipboard) {
      items.push({ label: `Paste (${clipboard.names.length} items)`, onClick: handlePaste });
    }
    items.push(
      { label: "Rename", onClick: () => handleStartRename(entry), divider: true },
      { label: "Delete", onClick: () => handleDelete(entry), danger: true, divider: true },
    );
    return items;
  };

  const handleKeyDown = (e: React.KeyboardEvent, entry: FileEntry) => {
    if (e.key === "Enter") {
      if (renamingEntry) return;
      handleOpen(entry);
    }
    if (e.key === "F2") {
      e.preventDefault();
      handleStartRename(entry);
    }
    if (e.key === "Delete") handleDelete(entry);
  };

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center text-gray-500">
        <svg className="w-5 h-5 animate-spin mr-2" fill="none" viewBox="0 0 24 24">
          <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
          <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
        </svg>
        Loading...
      </div>
    );
  }

  if (sorted.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-gray-500 text-sm">
        {searchResults ? "No files match your search" : "This folder is empty"}
      </div>
    );
  }

  const renderName = (entry: FileEntry) => {
    if (renamingEntry === entry.name) {
      return (
        <input
          ref={renameInputRef}
          value={renameValue}
          onChange={(e) => setRenameValue(e.target.value)}
          onBlur={handleRenameSubmit}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleRenameSubmit();
            if (e.key === "Escape") setRenamingEntry(null);
            e.stopPropagation();
          }}
          onClick={(e) => e.stopPropagation()}
          onDoubleClick={(e) => e.stopPropagation()}
          className="px-1 py-0.5 bg-gray-800 border border-indigo-500 rounded text-sm text-white w-full focus:outline-none"
        />
      );
    }
    return <span className="text-gray-200 truncate">{entry.name}</span>;
  };

  if (viewMode === "grid") {
    return (
      <div className="flex-1 overflow-auto p-3" onClick={() => setContextMenu(null)}>
        <div className="grid grid-cols-[repeat(auto-fill,minmax(100px,1fr))] gap-2">
          {sorted.map((entry) => (
            <button
              key={entry.name}
              className={`flex flex-col items-center gap-1 p-3 rounded-lg text-center transition ${
                selectedFiles.has(entry.name)
                  ? "bg-indigo-900/50 ring-1 ring-indigo-500"
                  : "hover:bg-gray-800"
              }`}
              onClick={(e) => toggleSelection(entry.name, e.metaKey || e.ctrlKey)}
              onDoubleClick={() => handleOpen(entry)}
              onContextMenu={(e) => handleContextMenu(e, entry)}
              onKeyDown={(e) => handleKeyDown(e, entry)}
            >
              <FileIcon isDir={entry.is_dir} size={32} name={entry.name} filePath={fullPath(entry.name)} />
              <span className="text-xs text-gray-300 truncate w-full">
                {renamingEntry === entry.name ? renderName(entry) : entry.name}
              </span>
            </button>
          ))}
        </div>
        {contextMenu && (
          <ContextMenu
            x={contextMenu.x}
            y={contextMenu.y}
            items={getContextMenuItems(contextMenu.entry)}
            onClose={() => setContextMenu(null)}
          />
        )}
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-auto" onClick={() => setContextMenu(null)}>
      <table className="w-full text-xs">
        <thead className="sticky top-0 bg-gray-900 z-10">
          <tr className="text-gray-400 text-left">
            <th className="py-1.5 px-3 font-medium cursor-pointer hover:text-white" onClick={() => setSortBy("name")}>
              Name {sortBy === "name" && (sortAsc ? "\u2191" : "\u2193")}
            </th>
            <th className="py-1.5 px-3 font-medium cursor-pointer hover:text-white w-24" onClick={() => setSortBy("size")}>
              Size {sortBy === "size" && (sortAsc ? "\u2191" : "\u2193")}
            </th>
            <th className="py-1.5 px-3 font-medium cursor-pointer hover:text-white w-44" onClick={() => setSortBy("modified")}>
              Modified {sortBy === "modified" && (sortAsc ? "\u2191" : "\u2193")}
            </th>
            <th className="py-1.5 px-3 w-16" />
          </tr>
        </thead>
        <tbody>
          {sorted.map((entry) => (
            <tr
              key={entry.name}
              className={`cursor-pointer border-b border-gray-800/50 ${
                selectedFiles.has(entry.name)
                  ? "bg-indigo-900/30"
                  : "hover:bg-gray-800/50"
              }`}
              onClick={(e) => toggleSelection(entry.name, e.metaKey || e.ctrlKey)}
              onDoubleClick={() => handleOpen(entry)}
              onContextMenu={(e) => handleContextMenu(e, entry)}
              onKeyDown={(e) => handleKeyDown(e, entry)}
              tabIndex={0}
            >
              <td className="py-1.5 px-3">
                <div className="flex items-center gap-2">
                  <FileIcon isDir={entry.is_dir} size={16} name={entry.name} filePath={fullPath(entry.name)} />
                  {renderName(entry)}
                </div>
              </td>
              <td className="py-1.5 px-3 text-[11px] text-gray-500">
                {entry.is_dir ? "--" : formatFileSize(entry.size)}
              </td>
              <td className="py-1.5 px-3 text-[11px] text-gray-500">
                {formatDate(entry.modified)}
              </td>
              <td className="py-1.5 px-3">
                <button
                  onClick={(e) => { e.stopPropagation(); handleDelete(entry); }}
                  className="text-gray-600 hover:text-red-400 p-1 rounded hover:bg-gray-800"
                  title="Delete"
                >
                  <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={getContextMenuItems(contextMenu.entry)}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif"]);
const VIDEO_EXTS = new Set(["mp4", "m4v", "mov", "webm", "ogg", "ogv", "avi", "mkv", "3gp", "ts"]);

function FileIcon({ isDir, size, name, filePath }: { isDir: boolean; size: number; name: string; filePath?: string }) {
  const s = size === 32 ? "w-8 h-8" : "w-4 h-4 shrink-0";
  const ext = name.split(".").pop()?.toLowerCase() ?? "";

  if (isDir) {
    return (
      <svg className={`${s} text-indigo-400`} fill="currentColor" viewBox="0 0 24 24">
        <path d="M10 4H4a2 2 0 00-2 2v12a2 2 0 002 2h16a2 2 0 002-2V8a2 2 0 00-2-2h-8l-2-2z" />
      </svg>
    );
  }

  // Image thumbnail
  if (IMAGE_EXTS.has(ext) && filePath) {
    const thumbSize = size === 32 ? "w-8 h-8" : "w-4 h-4 shrink-0";
    return (
      <img
        src={`vaultmedia://localhost/${encodeURIComponent(filePath)}`}
        alt=""
        className={`${thumbSize} object-cover rounded-sm`}
        loading="lazy"
        onError={(e) => {
          // Fall back to generic icon on error
          (e.target as HTMLImageElement).style.display = "none";
          (e.target as HTMLImageElement).nextElementSibling?.classList.remove("hidden");
        }}
      />
    );
  }

  // Video icon
  if (VIDEO_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-purple-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z" />
      </svg>
    );
  }

  return (
    <svg className={`${s} text-gray-500`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
    </svg>
  );
}
