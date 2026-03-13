import { create } from "zustand";
import type { VaultInfo } from "../hooks/useTauriCommands";

interface VaultStore {
  status: "locked" | "unlocked";
  vaultInfo: VaultInfo | null;
  recentVaults: string[];
  error: string | null;

  setUnlocked: (info: VaultInfo) => void;
  setLocked: () => void;
  setError: (error: string | null) => void;
  addRecentVault: (path: string) => void;
}

export const useVaultStore = create<VaultStore>((set) => ({
  status: "locked",
  vaultInfo: null,
  recentVaults: JSON.parse(localStorage.getItem("recentVaults") || "[]"),
  error: null,

  setUnlocked: (info) =>
    set(() => ({
      status: "unlocked",
      vaultInfo: info,
      error: null,
    })),

  setLocked: () => {
    localStorage.removeItem("recentVaults");
    return set(() => ({
      status: "locked",
      vaultInfo: null,
      error: null,
      recentVaults: [],
    }));
  },

  setError: (error) => set({ error }),

  addRecentVault: (path) =>
    set((state) => {
      const updated = [path, ...state.recentVaults.filter((p) => p !== path)].slice(0, 10);
      localStorage.setItem("recentVaults", JSON.stringify(updated));
      return { recentVaults: updated };
    }),
}));
