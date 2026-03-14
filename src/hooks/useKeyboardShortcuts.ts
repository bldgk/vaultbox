import { useEffect } from "react";
import { useFileStore } from "../store/fileStore";
import { useVaultStore } from "../store/vaultStore";
import { copyEntry, deleteEntry } from "./useTauriCommands";

export function useKeyboardShortcuts() {
  const { status } = useVaultStore();

  useEffect(() => {
    if (status !== "unlocked") return;

    const handler = async (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;

      const state = useFileStore.getState();

      // Cmd+N — new file dialog (handled via Toolbar, but we trigger it here)
      if (e.key === "n") {
        e.preventDefault();
        // Dispatch a custom event that Toolbar listens for
        window.dispatchEvent(new CustomEvent("vault:new-file"));
        return;
      }

      // Cmd+C — copy selected files
      if (e.key === "c") {
        const selected = Array.from(state.selectedFiles);
        if (selected.length === 0) return;
        e.preventDefault();
        const fullPaths = selected.map((name) =>
          state.currentPath ? `${state.currentPath}/${name}` : name
        );
        state.setClipboard({
          files: fullPaths,
          names: selected,
          sourceDir: state.currentPath,
          operation: e.shiftKey ? "cut" : "copy",
        });
        return;
      }

      // Cmd+X — cut selected files
      if (e.key === "x") {
        const selected = Array.from(state.selectedFiles);
        if (selected.length === 0) return;
        e.preventDefault();
        const fullPaths = selected.map((name) =>
          state.currentPath ? `${state.currentPath}/${name}` : name
        );
        state.setClipboard({
          files: fullPaths,
          names: selected,
          sourceDir: state.currentPath,
          operation: "cut",
        });
        return;
      }

      // Cmd+V — paste files
      if (e.key === "v") {
        if (!state.clipboard) return;
        e.preventDefault();
        const { clipboard, currentPath } = state;
        state.startBusy(`Pasting ${clipboard.names.length} item(s)...`);
        try {
          for (let i = 0; i < clipboard.files.length; i++) {
            const sourcePath = clipboard.files[i];
            let destName = clipboard.names[i];

            // If pasting to same dir, add " copy" suffix
            if (clipboard.sourceDir === currentPath && clipboard.operation === "copy") {
              const dotIdx = destName.lastIndexOf(".");
              if (dotIdx > 0) {
                destName = destName.slice(0, dotIdx) + " copy" + destName.slice(dotIdx);
              } else {
                destName = destName + " copy";
              }
            }

            await copyEntry(sourcePath, currentPath, destName);

            // If cut, delete the source
            if (clipboard.operation === "cut") {
              await deleteEntry(sourcePath, true);
            }
          }
          if (clipboard.operation === "cut") {
            state.setClipboard(null);
          }
          state.refresh();
        } catch (err) {
          alert(`Paste failed: ${err}`);
        } finally {
          state.stopBusy();
        }
        return;
      }

      // Cmd+A — select all
      if (e.key === "a") {
        e.preventDefault();
        const allNames = new Set(state.entries.map((e) => e.name));
        state.setSelectedFiles(allNames);
        return;
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [status]);
}
