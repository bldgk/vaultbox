import { useState } from "react";
import { useVaultStore } from "../../store/vaultStore";
import { openVault, createVault } from "../../hooks/useTauriCommands";
import { open } from "@tauri-apps/plugin-dialog";

export function UnlockDialog() {
  const [mode, setMode] = useState<"open" | "create">("open");
  const [path, setPath] = useState("");
  const [password, setPassword] = useState("");
  const [configPath, setConfigPath] = useState("");
  const [useExternalConfig, setUseExternalConfig] = useState(false);
  const [loading, setLoading] = useState(false);
  const { error, setError, setUnlocked, addRecentVault, recentVaults } = useVaultStore();

  const handleSelectFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      setPath(selected as string);
    }
  };

  const handleSelectConfig = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "gocryptfs config", extensions: ["conf"] }],
    });
    if (selected) {
      setConfigPath(selected as string);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!path || !password) return;

    setLoading(true);
    setError(null);

    try {
      const externalConf = useExternalConfig && configPath ? configPath : undefined;
      const info = mode === "open"
        ? await openVault(path, password, externalConf)
        : await createVault(path, password);
      addRecentVault(path);
      setUnlocked(info);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex items-center justify-center min-h-screen bg-gray-950">
      <div className="w-full max-w-md p-8 bg-gray-900 rounded-2xl shadow-2xl border border-gray-800">
        <div className="flex items-center gap-3 mb-6">
          <div className="w-10 h-10 bg-indigo-600 rounded-lg flex items-center justify-center">
            <svg className="w-6 h-6 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
            </svg>
          </div>
          <h1 className="text-xl font-semibold text-white">VaultBox</h1>
        </div>

        <div className="flex gap-2 mb-6">
          <button
            className={`flex-1 py-2 px-4 rounded-lg text-sm font-medium transition ${
              mode === "open"
                ? "bg-indigo-600 text-white"
                : "bg-gray-800 text-gray-400 hover:text-white"
            }`}
            onClick={() => setMode("open")}
          >
            Open Vault
          </button>
          <button
            className={`flex-1 py-2 px-4 rounded-lg text-sm font-medium transition ${
              mode === "create"
                ? "bg-indigo-600 text-white"
                : "bg-gray-800 text-gray-400 hover:text-white"
            }`}
            onClick={() => setMode("create")}
          >
            Create Vault
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm text-gray-400 mb-1">Vault Location</label>
            <div className="flex gap-2">
              <input
                type="text"
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="/path/to/vault"
                className="flex-1 px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white text-sm placeholder-gray-500 focus:outline-none focus:border-indigo-500"
              />
              <button
                type="button"
                onClick={handleSelectFolder}
                className="px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-gray-300 hover:text-white text-sm"
              >
                Browse
              </button>
            </div>
          </div>

          <div>
            <label className="block text-sm text-gray-400 mb-1">Password</label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Enter vault password"
              className="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white text-sm placeholder-gray-500 focus:outline-none focus:border-indigo-500"
            />
          </div>

          {mode === "open" && (
            <div>
              <label className="flex items-center gap-2 text-sm text-gray-400 cursor-pointer">
                <input
                  type="checkbox"
                  checked={useExternalConfig}
                  onChange={(e) => {
                    setUseExternalConfig(e.target.checked);
                    if (!e.target.checked) setConfigPath("");
                  }}
                  className="rounded border-gray-600 bg-gray-800 text-indigo-500 focus:ring-indigo-500 focus:ring-offset-0"
                />
                Use external gocryptfs.conf
              </label>
              {useExternalConfig && (
                <div className="flex gap-2 mt-2">
                  <input
                    type="text"
                    value={configPath}
                    onChange={(e) => setConfigPath(e.target.value)}
                    placeholder="/path/to/gocryptfs.conf"
                    className="flex-1 px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-white text-sm placeholder-gray-500 focus:outline-none focus:border-indigo-500"
                  />
                  <button
                    type="button"
                    onClick={handleSelectConfig}
                    className="px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-gray-300 hover:text-white text-sm"
                  >
                    Browse
                  </button>
                </div>
              )}
            </div>
          )}

          {error && (
            <div className="p-3 bg-red-900/50 border border-red-800 rounded-lg text-red-300 text-sm">
              {error}
            </div>
          )}

          <button
            type="submit"
            disabled={loading || !path || !password}
            className="w-full py-2.5 bg-indigo-600 text-white rounded-lg font-medium text-sm hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition"
          >
            {loading ? "Processing..." : mode === "open" ? "Unlock Vault" : "Create Vault"}
          </button>
        </form>

        {recentVaults.length > 0 && mode === "open" && (
          <div className="mt-6">
            <h3 className="text-sm text-gray-400 mb-2">Recent Vaults</h3>
            <div className="space-y-1">
              {recentVaults.map((vault) => (
                <button
                  key={vault}
                  className="w-full text-left px-3 py-2 bg-gray-800 rounded-lg text-gray-300 text-sm hover:bg-gray-700 truncate"
                  onClick={() => setPath(vault)}
                >
                  {vault}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
