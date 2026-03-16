import { useState, useEffect } from "react";
import { useVaultStore } from "../store/vaultStore";
import { useFileStore } from "../store/fileStore";

export function VaultStatusBar() {
  const { vaultInfo } = useVaultStore();
  const { entries, openTabs, clipboard, busyCount, statusText } = useFileStore();
  const [idleTime, setIdleTime] = useState(0);

  useEffect(() => {
    let lastActivity = Date.now();
    const events = ["mousedown", "keydown", "mousemove", "scroll"];
    const reset = () => { lastActivity = Date.now(); };
    events.forEach((e) => window.addEventListener(e, reset, { passive: true }));

    const interval = setInterval(() => {
      setIdleTime(Math.floor((Date.now() - lastActivity) / 1000));
    }, 1000);

    return () => {
      events.forEach((e) => window.removeEventListener(e, reset));
      clearInterval(interval);
    };
  }, []);

  const autoLockRemaining = Math.max(0, 600 - idleTime);
  const minutes = Math.floor(autoLockRemaining / 60);
  const seconds = autoLockRemaining % 60;
  const isBusy = busyCount > 0;

  return (
    <div className="flex items-center gap-4 px-3 py-1 bg-gray-900 border-t border-gray-800 text-xs text-gray-500" role="status" aria-live="polite" aria-label="Vault status bar">
      <span className="flex items-center gap-1.5">
        <span className={`w-1.5 h-1.5 rounded-full ${isBusy ? "bg-amber-400 animate-pulse" : "bg-green-500"}`} aria-hidden="true" />
        {isBusy ? (statusText || "Working...") : "Unlocked"}
      </span>
      {vaultInfo && (
        <span className="truncate max-w-[300px]" title={vaultInfo.path}>
          {vaultInfo.path}
        </span>
      )}
      <div className="flex-1" />
      {clipboard && (
        <span className="text-indigo-400">
          {clipboard.names.length} {clipboard.operation === "cut" ? "cut" : "copied"}
        </span>
      )}
      {openTabs.length > 0 && (
        <span>{openTabs.length} open tab{openTabs.length !== 1 ? "s" : ""}</span>
      )}
      <span>{entries.length} items</span>
      <span className={autoLockRemaining < 60 ? "text-yellow-500" : ""}>
        Auto-lock {minutes}:{seconds.toString().padStart(2, "0")}
      </span>
    </div>
  );
}
