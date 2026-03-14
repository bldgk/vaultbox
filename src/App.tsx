import { useVaultStore } from "./store/vaultStore";
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
  useAutoLock();
  useKeyboardShortcuts();

  if (status === "locked") {
    return <UnlockDialog />;
  }

  return (
    <div className="flex flex-col h-screen bg-gray-950 text-white select-none">
      <Toolbar />
      <Breadcrumb />
      <div className="flex flex-1 overflow-hidden">
        <FileTree />
        <FileList />
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
