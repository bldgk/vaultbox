import { useVaultStore } from "./store/vaultStore";
import { UnlockDialog } from "./components/dialogs/UnlockDialog";
import { Toolbar } from "./components/Toolbar";
import { Breadcrumb } from "./components/Breadcrumb";
import { FileTree } from "./components/FileTree";
import { FileList } from "./components/FileList";
import { ViewerPanel } from "./components/ViewerPanel";
import { VaultStatusBar } from "./components/VaultStatusBar";
import { FullscreenViewer } from "./components/FullscreenViewer";
import { useAutoLock } from "./hooks/useAutoLock";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import "./App.css";

function App() {
  const { status } = useVaultStore();
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
        <ViewerPanel />
      </div>
      <VaultStatusBar />
      <FullscreenViewer />
    </div>
  );
}

export default App;
