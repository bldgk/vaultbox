import { useEffect } from "react";
import { useFileStore } from "../store/fileStore";
import { useVaultStore } from "../store/vaultStore";
import { useDialogStore } from "../store/dialogStore";
import { copyEntry, deleteEntry, createFile } from "./useTauriCommands";

export function useKeyboardShortcuts() {
  const { status } = useVaultStore();

  useEffect(() => {
    if (status !== "unlocked") return;

    const handler = async (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;

      const state = useFileStore.getState();

      // Cmd+Shift+N — new folder dialog
      if (e.key === "N" && e.shiftKey) {
        e.preventDefault();
        window.dispatchEvent(new CustomEvent("vault:new-folder"));
        return;
      }

      // Cmd+N — create new untitled.txt immediately
      if (e.key === "n" && !e.shiftKey) {
        e.preventDefault();
        const { currentPath } = state;
        // Find unique name
        const existingNames = new Set(state.entries.map((en) => en.name));
        let name = "untitled.txt";
        let counter = 1;
        while (existingNames.has(name)) {
          name = `untitled${counter}.txt`;
          counter++;
        }
        try {
          await createFile(currentPath, name);
          state.refresh();
        } catch {
          // silently fail
        }
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
          useDialogStore.getState().showConfirm({
            title: "Error",
            message: `Paste failed: ${err}`,
            confirmLabel: "OK",
            onConfirm: () => {},
          });
        } finally {
          state.stopBusy();
        }
        return;
      }

      // Cmd+I — toggle file info panel
      if (e.key === "i") {
        e.preventDefault();
        state.toggleInfoPanel();
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
