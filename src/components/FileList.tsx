import { useEffect, useCallback, useState, useRef } from "react";
import { useFileStore } from "../store/fileStore";
import { useDialogStore } from "../store/dialogStore";
import { listDir, readFile, deleteEntry, renameEntry, exportFile, copyEntry, importFiles } from "../hooks/useTauriCommands";
import type { FileEntry } from "../hooks/useTauriCommands";
import { formatFileSize, formatDate, getViewerType } from "../lib/fileTypes";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import { save, open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";

export function FileList() {
  const {
    currentPath, refreshCounter, entries, setEntries, selectedFiles, toggleSelection,
    navigateTo, openTab, viewMode, sortBy, sortAsc, setSortBy,
    searchResults, loading, setLoading, clipboard, setClipboard, setFullscreenPreview,
    startBusy, stopBusy, toggleInfoPanel, setSelectedFiles, lastClickedFile,
  } = useFileStore();

  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; entry: FileEntry } | null>(null);
  const [renamingEntry, setRenamingEntry] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const renameInputRef = useRef<HTMLInputElement>(null);
  const [isDragOver, setIsDragOver] = useState(false);

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

  // Tauri drag-and-drop: import files dropped onto the window
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let lastDropTime = 0;

    getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "enter" || event.payload.type === "over") {
        setIsDragOver(true);
      } else if (event.payload.type === "drop") {
        setIsDragOver(false);
        const now = Date.now();
        if (now - lastDropTime < 1000) return;
        lastDropTime = now;
        const paths = event.payload.paths;
        if (paths.length > 0) {
          const { currentPath } = useFileStore.getState();
          startBusy(`Importing ${paths.length} file(s)...`);
          importFiles(paths, currentPath)
            .then(() => useFileStore.getState().refresh())
            .catch((err) => {
              useDialogStore.getState().showConfirm({
                title: "Error",
                message: `Import failed: ${err}`,
                confirmLabel: "OK",
                onConfirm: () => {},
              });
            })
            .finally(() => stopBusy());
        }
      } else if (event.payload.type === "leave") {
        setIsDragOver(false);
      }
    }).then((fn) => { unlisten = fn; });

    return () => { unlisten?.(); };
  }, []);

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

  // Unified click handler: shift=range, ctrl/cmd=toggle, plain=single select
  const handleItemClick = (e: React.MouseEvent, name: string) => {
    if (e.shiftKey && lastClickedFile) {
      // Range select from lastClickedFile to name
      const anchorIdx = sorted.findIndex((f) => f.name === lastClickedFile);
      const targetIdx = sorted.findIndex((f) => f.name === name);
      if (anchorIdx >= 0 && targetIdx >= 0) {
        const start = Math.min(anchorIdx, targetIdx);
        const end = Math.max(anchorIdx, targetIdx);
        const newSet = new Set(e.ctrlKey || e.metaKey ? selectedFiles : []);
        for (let i = start; i <= end; i++) {
          newSet.add(sorted[i].name);
        }
        setSelectedFiles(newSet);
        // Don't update lastClickedFile on shift-click (keep anchor)
        return;
      }
    }
    toggleSelection(name, e.metaKey || e.ctrlKey);
  };


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
        showError(`Failed to read file: ${err}`);
      } finally {
        stopBusy();
      }
    }
  };

  const { showConfirm } = useDialogStore();

  const showError = (msg: string) => {
    showConfirm({
      title: "Error",
      message: msg,
      confirmLabel: "OK",
      danger: false,
      onConfirm: () => {},
    });
  };

  // Close any open tabs whose path starts with the deleted path
  const closeTabsForPath = (deletedPath: string) => {
    const state = useFileStore.getState();
    const toClose = state.openTabs
      .map((t, i) => ({ path: t.path, index: i }))
      .filter((t) => t.path === deletedPath || t.path.startsWith(deletedPath + "/"))
      .reverse(); // close from end to avoid index shift
    for (const t of toClose) {
      useFileStore.getState().closeTab(t.index);
    }
  };

  const handleDelete = (entry: FileEntry) => {
    showConfirm({
      title: "Delete File",
      message: `Delete "${entry.name}" permanently? This cannot be undone.`,
      confirmLabel: "Delete",
      danger: true,
      onConfirm: async () => {
        try {
          await deleteEntry(fullPath(entry.name), true);
          closeTabsForPath(fullPath(entry.name));
          useFileStore.getState().refresh();
        } catch (err) {
          showError(`Failed to delete: ${err}`);
        }
      },
    });
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
      showError(`Failed to rename: ${err}`);
    }
  };

  const handleExport = async (entry: FileEntry) => {
    try {
      const dest = await save({ defaultPath: entry.name });
      if (dest) {
        await exportFile(fullPath(entry.name), dest);

      }
    } catch (err) {
      showError(`Failed to export: ${err}`);
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
      showError(`Paste failed: ${err}`);
    } finally {
      stopBusy();
    }
  };

  // --- Move files to folder (via context menu) ---
  const handleMoveToFolder = async (fileNames: string[], folderName: string) => {
    const destDir = fullPath(folderName);
    startBusy(`Moving ${fileNames.length} item(s) to ${folderName}...`);
    try {
      for (const name of fileNames) {
        await copyEntry(fullPath(name), destDir, name);
        await deleteEntry(fullPath(name), true);
      }

      setSelectedFiles(new Set());
      useFileStore.getState().refresh();
    } catch (err) {
      showError(`Move failed: ${err}`);
    } finally {
      stopBusy();
    }
  };

  const handlePreview = async (entry: FileEntry) => {
    const fp = fullPath(entry.name);
    // Reuse content from open tab if available
    const { openTabs } = useFileStore.getState();
    const existingTab = openTabs.find((t) => t.path === fp);
    if (existingTab?.content) {
      setFullscreenPreview({ filePath: fp, fileName: entry.name, content: existingTab.content });
      return;
    }
    // Otherwise load — but for images/video, fullscreen uses vaultmedia:// so content is just a placeholder
    try {
      const content = await readFile(fp);
      setFullscreenPreview({ filePath: fp, fileName: entry.name, content });
    } catch (err) {
      showError(`Failed to preview: ${err}`);
    }
  };

  // --- Batch operations ---
  const getSelectedEntries = (): FileEntry[] => {
    return sorted.filter((e) => selectedFiles.has(e.name));
  };

  const handleBatchDelete = () => {
    const selected = getSelectedEntries();
    showConfirm({
      title: "Delete Selected Files",
      message: `Delete ${selected.length} item(s) permanently? This cannot be undone.`,
      confirmLabel: "Delete All",
      danger: true,
      onConfirm: async () => {
        startBusy(`Deleting ${selected.length} item(s)...`);
        try {
          for (const entry of selected) {
            await deleteEntry(fullPath(entry.name), true);
            closeTabsForPath(fullPath(entry.name));
          }
          useFileStore.getState().setSelectedFiles(new Set());
          useFileStore.getState().refresh();
        } catch (err) {
          showError(`Failed to delete: ${err}`);
        } finally {
          stopBusy();
        }
      },
    });
  };

  const handleBatchExport = async () => {
    const selected = getSelectedEntries().filter((e) => !e.is_dir);
    if (selected.length === 0) return;

    if (selected.length === 1) {
      await handleExport(selected[0]);
      return;
    }

    // Multiple files: pick a destination folder
    const destFolder = await openDialog({ directory: true, title: "Choose export folder" });
    if (!destFolder) return;

    startBusy(`Exporting ${selected.length} file(s)...`);
    try {
      for (const entry of selected) {
        await exportFile(fullPath(entry.name), `${destFolder}/${entry.name}`);
      }

    } catch (err) {
      showError(`Failed to export: ${err}`);
    } finally {
      stopBusy();
    }
  };

  const handleBatchCopy = () => {
    const selected = getSelectedEntries();
    setClipboard({
      files: selected.map((e) => fullPath(e.name)),
      names: selected.map((e) => e.name),
      sourceDir: currentPath,
      operation: "copy",
    });
  };

  const handleBatchMoveTo = (folderName: string) => {
    const selected = getSelectedEntries().filter((e) => e.name !== folderName);
    if (selected.length === 0) return;
    handleMoveToFolder(selected.map((e) => e.name), folderName);
  };

  const availableFolders = sorted.filter((e) => e.is_dir && !selectedFiles.has(e.name));

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
    if (!entry.is_dir) {
      items.push({
        label: "File Info",
        onClick: () => {
          setSelectedFiles(new Set([entry.name]));
          if (!useFileStore.getState().showInfoPanel) toggleInfoPanel();
        },
      });
    }
    items.push(
      { label: "Copy", onClick: () => handleCopy(entry), divider: true },
    );
    if (clipboard) {
      items.push({ label: `Paste (${clipboard.names.length} items)`, onClick: handlePaste });
    }
    // "Move to" flyout submenu — lists folders in current directory
    const folders = sorted.filter((e) => e.is_dir && e.name !== entry.name);
    if (!entry.is_dir && folders.length > 0) {
      const targetNames = selectedFiles.size > 1 && selectedFiles.has(entry.name)
        ? Array.from(selectedFiles)
        : [entry.name];
      items.push({
        label: "Move to",
        divider: true,
        children: folders.map((folder) => ({
          label: folder.name,
          onClick: () => handleMoveToFolder(targetNames, folder.name),
        })),
      });
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
      <div className="flex-1 flex items-center justify-center text-gray-500 min-w-0">
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
      <div className="flex-1 flex items-center justify-center text-gray-500 text-sm relative min-w-0">
        {isDragOver ? (
          <DragOverlay />
        ) : (
          searchResults ? "No files match your search" : "Drop files here or use Import"
        )}
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
      <div className="flex-1 flex flex-col overflow-hidden relative min-w-0">
        {selectedFiles.size > 0 && (
          <BatchActionBar
            count={selectedFiles.size}
            onExport={handleBatchExport}
            onDelete={handleBatchDelete}
            onCopy={handleBatchCopy}
            folders={availableFolders}
            onMoveTo={handleBatchMoveTo}
            onDeselect={() => setSelectedFiles(new Set())}
          />
        )}
        <div className="flex-1 overflow-auto p-3" onClick={() => setContextMenu(null)}>
          {isDragOver && <DragOverlay />}
          <div className="grid grid-cols-[repeat(auto-fill,minmax(100px,1fr))] gap-2">
            {sorted.map((entry) => (
              <div
                key={entry.name}
                className={`relative flex flex-col items-center gap-1 p-3 rounded-lg text-center transition cursor-pointer ${
                  selectedFiles.has(entry.name)
                    ? "bg-indigo-900/50 ring-1 ring-indigo-500"
                    : "hover:bg-gray-800"
                }`}
                onClick={(e) => handleItemClick(e, entry.name)}
                onDoubleClick={(e) => {
                  if ((e.target as HTMLElement).closest("input[type=checkbox]")) return;
                  handleOpen(entry);
                }}
                onContextMenu={(e) => handleContextMenu(e, entry)}
                onKeyDown={(e) => handleKeyDown(e, entry)}
                tabIndex={0}
              >
                <input
                  type="checkbox"
                  checked={selectedFiles.has(entry.name)}
                  onChange={() => toggleSelection(entry.name, true)}
                  onClick={(e) => e.stopPropagation()}
                  className={`absolute top-1.5 left-1.5 w-3.5 h-3.5 rounded border-gray-600 bg-gray-800 text-indigo-500 focus:ring-0 cursor-pointer accent-indigo-500 ${
                    selectedFiles.has(entry.name) || selectedFiles.size > 0 ? "opacity-100" : "opacity-0 hover:opacity-100"
                  }`}
                />
                <FileIcon isDir={entry.is_dir} size={32} name={entry.name} filePath={fullPath(entry.name)} />
                <span className="text-xs text-gray-300 truncate w-full">
                  {renamingEntry === entry.name ? renderName(entry) : entry.name}
                </span>
              </div>
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
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col overflow-hidden relative min-w-0">
      {selectedFiles.size > 0 && (
        <BatchActionBar
          count={selectedFiles.size}
          onExport={handleBatchExport}
          onDelete={handleBatchDelete}
          onCopy={handleBatchCopy}
          folders={availableFolders}
          onMoveTo={handleBatchMoveTo}
          onDeselect={() => setSelectedFiles(new Set())}
        />
      )}
      <div className="flex-1 overflow-auto" onClick={() => setContextMenu(null)}>
        {isDragOver && <DragOverlay />}
        <table className="w-full text-xs table-fixed">
          <thead className="sticky top-0 bg-gray-900 z-10">
            <tr className="text-gray-400 text-left">
              <th className="py-1.5 px-1.5 w-8">
                <input
                  type="checkbox"
                  checked={sorted.length > 0 && selectedFiles.size === sorted.length}
                  ref={(el) => { if (el) el.indeterminate = selectedFiles.size > 0 && selectedFiles.size < sorted.length; }}
                  onChange={() => {
                    if (selectedFiles.size === sorted.length) setSelectedFiles(new Set());
                    else setSelectedFiles(new Set(sorted.map((e) => e.name)));
                  }}
                  className="w-3.5 h-3.5 rounded border-gray-600 bg-gray-800 text-indigo-500 focus:ring-0 cursor-pointer accent-indigo-500"
                  title="Select all"
                />
              </th>
              <th className="py-1.5 px-3 font-medium cursor-pointer hover:text-white truncate" onClick={() => setSortBy("name")}>
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
                onClick={(e) => handleItemClick(e, entry.name)}
                onDoubleClick={(e) => {
                  if ((e.target as HTMLElement).closest("button, input[type=checkbox]")) return;
                  handleOpen(entry);
                }}
                onContextMenu={(e) => handleContextMenu(e, entry)}
                onKeyDown={(e) => handleKeyDown(e, entry)}
                tabIndex={0}
              >
                <td className="py-1.5 px-1.5 w-8" onClick={(e) => e.stopPropagation()}>
                  <input
                    type="checkbox"
                    checked={selectedFiles.has(entry.name)}
                    onChange={() => toggleSelection(entry.name, true)}
                    className="w-3.5 h-3.5 rounded border-gray-600 bg-gray-800 text-indigo-500 focus:ring-0 cursor-pointer accent-indigo-500"
                  />
                </td>
                <td className="py-1.5 px-3 truncate">
                  <div className="flex items-center gap-2 min-w-0">
                    <FileIcon isDir={entry.is_dir} size={16} name={entry.name} filePath={fullPath(entry.name)} />
                    <div className="truncate">{renderName(entry)}</div>
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
    </div>
  );
}

function BatchActionBar({ count, onExport, onDelete, onCopy, folders, onMoveTo, onDeselect }: {
  count: number;
  onExport: () => void;
  onDelete: () => void;
  onCopy: () => void;
  folders: FileEntry[];
  onMoveTo: (folderName: string) => void;
  onDeselect: () => void;
}) {
  const [showMoveMenu, setShowMoveMenu] = useState(false);
  const moveRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!showMoveMenu) return;
    const handler = (e: MouseEvent) => {
      if (moveRef.current && !moveRef.current.contains(e.target as Node)) setShowMoveMenu(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [showMoveMenu]);

  return (
    <div className="flex items-center gap-3 px-3 py-2 bg-indigo-950/60 border-b border-indigo-800/50">
      <span className="text-xs text-indigo-300 font-medium">{count} items selected</span>
      <button onClick={onDeselect} className="text-gray-500 hover:text-gray-300 p-0.5" title="Deselect all">
        <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
      <div className="flex gap-1.5 ml-auto">
        <button
          onClick={onExport}
          className="flex items-center gap-1.5 px-2.5 py-1 text-xs text-gray-300 bg-gray-800 hover:bg-gray-700 rounded-md transition"
          title="Export selected files"
        >
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
          </svg>
          Export
        </button>
        <button
          onClick={onCopy}
          className="flex items-center gap-1.5 px-2.5 py-1 text-xs text-gray-300 bg-gray-800 hover:bg-gray-700 rounded-md transition"
          title="Copy selected files to clipboard"
        >
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
          </svg>
          Copy
        </button>
        {folders.length > 0 && (
          <div ref={moveRef} className="relative">
            <button
              onClick={() => setShowMoveMenu((v) => !v)}
              className="flex items-center gap-1.5 px-2.5 py-1 text-xs text-gray-300 bg-gray-800 hover:bg-gray-700 rounded-md transition"
              title="Move selected files to folder"
            >
              <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
              </svg>
              Move to
              <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
              </svg>
            </button>
            {showMoveMenu && (
              <div className="absolute right-0 top-full mt-1 bg-gray-800 border border-gray-700 rounded-lg shadow-xl py-1 z-50 min-w-[140px] max-h-48 overflow-y-auto">
                {folders.map((f) => (
                  <button
                    key={f.name}
                    className="w-full text-left px-3 py-1.5 text-xs text-gray-300 hover:bg-gray-700 flex items-center gap-2"
                    onClick={() => { onMoveTo(f.name); setShowMoveMenu(false); }}
                  >
                    <svg className="w-3.5 h-3.5 text-indigo-400 shrink-0" fill="currentColor" viewBox="0 0 24 24">
                      <path d="M10 4H4a2 2 0 00-2 2v12a2 2 0 002 2h16a2 2 0 002-2V8a2 2 0 00-2-2h-8l-2-2z" />
                    </svg>
                    <span className="truncate">{f.name}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
        <button
          onClick={onDelete}
          className="flex items-center gap-1.5 px-2.5 py-1 text-xs text-red-400 bg-red-950/50 hover:bg-red-900/50 rounded-md transition"
          title="Delete selected files"
        >
          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
          Delete
        </button>
      </div>
    </div>
  );
}

function DragOverlay() {
  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center bg-indigo-950/80 border-2 border-dashed border-indigo-400 rounded-lg m-2 pointer-events-none">
      <div className="text-center">
        <svg className="w-12 h-12 mx-auto mb-3 text-indigo-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
        </svg>
        <p className="text-indigo-300 text-sm font-medium">Drop files to import</p>
        <p className="text-indigo-400/60 text-xs mt-1">Files will be encrypted into the vault</p>
      </div>
    </div>
  );
}

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif"]);
const VIDEO_EXTS = new Set(["mp4", "m4v", "mov", "webm", "ogg", "ogv", "avi", "mkv", "3gp", "ts"]);
const CODE_EXTS = new Set(["js", "ts", "jsx", "tsx", "py", "rs", "go", "java", "c", "cpp", "rb", "php", "swift", "kt", "scala"]);
const TEXT_EXTS = new Set(["txt", "md", "log", "doc", "rtf"]);
const CONFIG_EXTS = new Set(["json", "yml", "yaml", "xml", "toml", "ini", "cfg", "conf", "env", "csv"]);
const MARKUP_EXTS = new Set(["html", "css", "scss", "vue", "svelte"]);
const AUDIO_EXTS = new Set(["mp3", "wav", "ogg", "m4a", "flac", "aac"]);
const ARCHIVE_EXTS = new Set(["zip", "tar", "gz", "rar", "7z", "bz2", "xz"]);
const SHELL_EXTS = new Set(["sh", "bash", "zsh", "fish", "bat", "ps1"]);

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

  // Code files — angle brackets with slash icon
  if (CODE_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-blue-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 16L4 12l4-4m8 8l4-4-4-4m-5-2l2 16" />
      </svg>
    );
  }

  // Text/docs — document with text lines icon
  if (TEXT_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-gray-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 13h6m-6 4h4m-4-8h2" />
      </svg>
    );
  }

  // Config/data — cog/settings icon
  if (CONFIG_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-amber-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.573-1.066z" />
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
      </svg>
    );
  }

  // Markup — globe icon
  if (MARKUP_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-orange-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
      </svg>
    );
  }

  // PDF — document with "PDF" label
  if (ext === "pdf") {
    return (
      <svg className={`${s} text-red-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
        <text x="12" y="16" textAnchor="middle" fill="currentColor" stroke="none" fontSize="7" fontWeight="bold" fontFamily="sans-serif">PDF</text>
      </svg>
    );
  }

  // Audio — music note icon
  if (AUDIO_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-pink-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 19V6l12-3v13" />
        <circle cx="6" cy="19" r="3" strokeWidth={1.5} fill="none" />
        <circle cx="18" cy="16" r="3" strokeWidth={1.5} fill="none" />
      </svg>
    );
  }

  // Archive — archive box icon
  if (ARCHIVE_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-yellow-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M5 8h14M5 8a2 2 0 01-2-2V5a2 2 0 012-2h14a2 2 0 012 2v1a2 2 0 01-2 2M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4" />
      </svg>
    );
  }

  // Shell/scripts — terminal icon
  if (SHELL_EXTS.has(ext)) {
    return (
      <svg className={`${s} text-green-400`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
      </svg>
    );
  }

  // Default generic file icon
  return (
    <svg className={`${s} text-gray-500`} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
    </svg>
  );
}
