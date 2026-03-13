import { useEffect, useRef } from "react";
import { useVaultStore } from "../store/vaultStore";
import { useFileStore } from "../store/fileStore";
import { lockVault } from "./useTauriCommands";

const AUTO_LOCK_MS = 10 * 60 * 1000; // 10 minutes

export function useAutoLock() {
  const { status, setLocked } = useVaultStore();
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const resetTimer = () => {
    if (timerRef.current) clearTimeout(timerRef.current);
    if (status === "unlocked") {
      timerRef.current = setTimeout(async () => {
        try {
          await lockVault();
          setLocked();
          useFileStore.getState().reset();
        } catch {
          // Vault may already be locked
        }
      }, AUTO_LOCK_MS);
    }
  };

  useEffect(() => {
    if (status !== "unlocked") {
      if (timerRef.current) clearTimeout(timerRef.current);
      return;
    }

    resetTimer();

    const events = ["mousedown", "keydown", "mousemove", "scroll", "touchstart"];
    const handler = () => resetTimer();
    events.forEach((e) => window.addEventListener(e, handler, { passive: true }));

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      events.forEach((e) => window.removeEventListener(e, handler));
    };
  }, [status]);
}
