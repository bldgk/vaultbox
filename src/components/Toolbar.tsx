import { useState, useEffect } from "react";
import { useFileStore } from "../store/fileStore";
import { useVaultStore } from "../store/vaultStore";
import { lockVault, createFile, createDir, searchFiles, importFiles } from "../hooks/useTauriCommands";
import { open } from "@tauri-apps/plugin-dialog";

export function Toolbar() {
  const { currentPath, goBack, goForward, historyIndex, navigationHistory, viewMode, setViewMode, searchQuery, setSearchQuery, setSearchResults } = useFileStore();
  const { setLocked } = useVaultStore();
  const [showNewMenu, setShowNewMenu] = useState(false);
  const [newName, setNewName] = useState("");
  const [newType, setNewType] = useState<"file" | "folder" | null>(null);

  // Listen for Cmd+N keyboard shortcut
  useEffect(() => {
    const handler = () => {
      setNewType("file");
      setShowNewMenu(false);
      setNewName("");
    };
    window.addEventListener("vault:new-file", handler);
    return () => window.removeEventListener("vault:new-file", handler);
  }, []);

  const handleLock = async () => {
    await lockVault();
    setLocked();
    useFileStore.getState().reset();
  };

  const handleNew = async (type: "file" | "folder") => {
    setNewType(type);
    setShowNewMenu(false);
    setNewName("");
  };

  const handleCreateSubmit = async () => {
    if (!newName.trim()) return;
    try {
      if (newType === "file") {
        await createFile(currentPath, newName);
      } else {
        await createDir(currentPath, newName);
      }
      setNewType(null);
      setNewName("");
      useFileStore.getState().refresh();
    } catch (err) {
      alert(String(err));
    }
  };

  const handleSearch = async () => {
    if (!searchQuery.trim()) {
      setSearchResults(null);
      return;
    }
    try {
      const results = await searchFiles(searchQuery);
      setSearchResults(results);
    } catch {
      // Search failed
    }
  };

  return (
    <div className="flex items-center gap-2 px-3 py-2 bg-gray-900 border-b border-gray-800">
      {/* Navigation */}
      <button
        onClick={goBack}
        disabled={historyIndex <= 0}
        className="p-1.5 rounded hover:bg-gray-800 disabled:opacity-30 text-gray-400"
        title="Back"
      >
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" /></svg>
      </button>
      <button
        onClick={goForward}
        disabled={historyIndex >= navigationHistory.length - 1}
        className="p-1.5 rounded hover:bg-gray-800 disabled:opacity-30 text-gray-400"
        title="Forward"
      >
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" /></svg>
      </button>

      {/* New file/folder */}
      <div className="relative">
        <button
          onClick={() => setShowNewMenu(!showNewMenu)}
          className="p-1.5 rounded hover:bg-gray-800 text-gray-400"
          title="New"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" /></svg>
        </button>
        {showNewMenu && (
          <div className="absolute top-full left-0 mt-1 bg-gray-800 border border-gray-700 rounded-lg shadow-xl z-10 py-1 w-36">
            <button onClick={() => handleNew("file")} className="w-full px-3 py-1.5 text-left text-sm text-gray-300 hover:bg-gray-700">New File</button>
            <button onClick={() => handleNew("folder")} className="w-full px-3 py-1.5 text-left text-sm text-gray-300 hover:bg-gray-700">New Folder</button>
          </div>
        )}
      </div>

      {/* Import */}
      <button
        onClick={async () => {
          const selected = await open({ multiple: true });
          if (selected) {
            const paths = Array.isArray(selected) ? selected : [selected];
            useFileStore.getState().startBusy();
            try {
              await importFiles(paths as string[], currentPath);
              useFileStore.getState().refresh();
            } catch (err) {
              alert(String(err));
            } finally {
              useFileStore.getState().stopBusy();
            }
          }
        }}
        className="p-1.5 rounded hover:bg-gray-800 text-gray-400"
        title="Import files"
      >
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" /></svg>
      </button>

      {/* View mode */}
      <button
        onClick={() => setViewMode(viewMode === "list" ? "grid" : "list")}
        className="p-1.5 rounded hover:bg-gray-800 text-gray-400"
        title={viewMode === "list" ? "Grid view" : "List view"}
      >
        {viewMode === "list" ? (
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" /></svg>
        ) : (
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" /></svg>
        )}
      </button>

      <div className="flex-1" />

      {/* Search */}
      <div className="flex gap-1">
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSearch()}
          placeholder="Search files..."
          className="px-2 py-1 bg-gray-800 border border-gray-700 rounded text-sm text-white placeholder-gray-500 w-48 focus:outline-none focus:border-indigo-500"
        />
      </div>

      {/* Lock */}
      <button
        onClick={handleLock}
        className="p-1.5 rounded hover:bg-red-900/50 text-gray-400 hover:text-red-400"
        title="Lock vault"
      >
        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" /></svg>
      </button>

      {/* New name dialog */}
      {newType && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-gray-900 border border-gray-700 rounded-xl p-4 w-80">
            <h3 className="text-white text-sm font-medium mb-3">
              New {newType === "file" ? "File" : "Folder"}
            </h3>
            <input
              autoFocus
              type="text"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleCreateSubmit();
                if (e.key === "Escape") setNewType(null);
              }}
              placeholder={newType === "file" ? "filename.txt" : "folder name"}
              className="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white text-sm placeholder-gray-500 focus:outline-none focus:border-indigo-500 mb-3"
            />
            <div className="flex gap-2 justify-end">
              <button onClick={() => setNewType(null)} className="px-3 py-1.5 text-sm text-gray-400 hover:text-white">Cancel</button>
              <button onClick={handleCreateSubmit} className="px-3 py-1.5 bg-indigo-600 text-white text-sm rounded-lg hover:bg-indigo-700">Create</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
