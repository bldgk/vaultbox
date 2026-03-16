import { useVaultStore } from "./store/vaultStore";
import { useFileStore } from "./store/fileStore";
import { useDialogStore } from "./store/dialogStore";
import { UnlockDialog } from "./components/dialogs/UnlockDialog";
import { ConfirmDialog } from "./components/dialogs/ConfirmDialog";
import { Toolbar } from "./components/Toolbar";
import { Breadcrumb } from "./components/Breadcrumb";
import { FileTree } from "./components/FileTree";
import { FileList } from "./components/FileList";
import { ViewerPanel } from "./components/ViewerPanel";
import { VaultStatusBar } from "./components/VaultStatusBar";
import { FullscreenViewer } from "./components/FullscreenViewer";
import { FileInfoPanel } from "./components/FileInfoPanel";
import { useAutoLock } from "./hooks/useAutoLock";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import "./App.css";

function App() {
  const { status } = useVaultStore();
  const { confirmDialog, hideConfirm } = useDialogStore();
  const { fileTreeCollapsed, toggleFileTree, fileListCollapsed, toggleFileList, openTabs } = useFileStore();
  useAutoLock();
  useKeyboardShortcuts();

  if (status === "locked") {
    return <UnlockDialog />;
  }

  const hasOpenTabs = openTabs.length > 0;

  return (
    <div className="flex flex-col h-screen bg-gray-950 text-white select-none">
      <Toolbar />
      <Breadcrumb />
      <div className="flex flex-1 overflow-hidden">
        {!fileTreeCollapsed && <FileTree />}
        <button
          onClick={toggleFileTree}
          className="shrink-0 w-5 flex items-center justify-center bg-gray-900 border-r border-gray-800 hover:bg-gray-800 transition-colors text-gray-500 hover:text-gray-300"
          title={fileTreeCollapsed ? "Show folders" : "Hide folders"}
        >
          <svg
            className={`w-3 h-3 transition-transform ${fileTreeCollapsed ? "rotate-180" : ""}`}
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        {/* Collapse/expand toggle — only show when tabs are open */}
        {hasOpenTabs && (
          <button
            onClick={toggleFileList}
            className="shrink-0 w-5 flex items-center justify-center bg-gray-900 border-x border-gray-800 hover:bg-gray-800 transition-colors text-gray-500 hover:text-gray-300"
            title={fileListCollapsed ? "Show file list" : "Hide file list"}
          >
            <svg
              className={`w-3 h-3 transition-transform ${fileListCollapsed ? "rotate-180" : ""}`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
            </svg>
          </button>
        )}
        {!(hasOpenTabs && fileListCollapsed) && <FileList />}
        <FileInfoPanel />
        <ViewerPanel />
      </div>
      <VaultStatusBar />
      <FullscreenViewer />
      {confirmDialog && (
        <ConfirmDialog
          open={confirmDialog.open}
          title={confirmDialog.title}
          message={confirmDialog.message}
          confirmLabel={confirmDialog.confirmLabel}
          danger={confirmDialog.danger}
          onConfirm={() => {
            confirmDialog.onConfirm();
            hideConfirm();
          }}
          onCancel={hideConfirm}
        />
      )}
    </div>
  );
}

export default App;
