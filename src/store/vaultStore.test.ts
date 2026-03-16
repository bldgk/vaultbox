import { describe, it, expect, beforeEach } from "vitest";
import { useVaultStore } from "./vaultStore";

describe("vaultStore", () => {
  beforeEach(() => {
    localStorage.clear();
    useVaultStore.setState({
      status: "locked",
      vaultInfo: null,
      error: null,
      recentVaults: [],
    });
  });

  it("starts in locked state", () => {
    const state = useVaultStore.getState();
    expect(state.status).toBe("locked");
    expect(state.vaultInfo).toBeNull();
    expect(state.error).toBeNull();
  });

  it("setUnlocked transitions to unlocked with vault info", () => {
    const info = {
      path: "/test/vault",
      version: 2,
      feature_flags: [],
    };
    useVaultStore.getState().setUnlocked(info);

    const state = useVaultStore.getState();
    expect(state.status).toBe("unlocked");
    expect(state.vaultInfo).toEqual(info);
    expect(state.error).toBeNull();
  });

  it("setLocked transitions back to locked and clears vault info", () => {
    useVaultStore.getState().setUnlocked({
      path: "/test/vault",
      version: 2,
      feature_flags: [],
    });
    useVaultStore.getState().setLocked();

    const state = useVaultStore.getState();
    expect(state.status).toBe("locked");
    expect(state.vaultInfo).toBeNull();
  });

  it("setError stores error message", () => {
    useVaultStore.getState().setError("Something went wrong");
    expect(useVaultStore.getState().error).toBe("Something went wrong");

    useVaultStore.getState().setError(null);
    expect(useVaultStore.getState().error).toBeNull();
  });

  it("addRecentVault adds and deduplicates paths", () => {
    useVaultStore.getState().addRecentVault("/vault/a");
    useVaultStore.getState().addRecentVault("/vault/b");
    useVaultStore.getState().addRecentVault("/vault/a"); // duplicate — should move to front

    const { recentVaults } = useVaultStore.getState();
    expect(recentVaults).toEqual(["/vault/a", "/vault/b"]);
  });

  it("addRecentVault limits to 10 entries", () => {
    for (let i = 0; i < 15; i++) {
      useVaultStore.getState().addRecentVault(`/vault/${i}`);
    }
    expect(useVaultStore.getState().recentVaults).toHaveLength(10);
  });
});
