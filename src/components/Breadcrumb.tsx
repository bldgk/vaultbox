import { useFileStore } from "../store/fileStore";

export function Breadcrumb() {
  const { currentPath, navigateTo } = useFileStore();

  const parts = currentPath ? currentPath.split("/").filter(Boolean) : [];

  return (
    <nav className="flex items-center gap-1 px-3 py-1.5 bg-gray-900/50 border-b border-gray-800 text-sm overflow-x-auto" aria-label="Breadcrumb">
      <button
        onClick={() => navigateTo("")}
        className="text-gray-400 hover:text-white px-1.5 py-0.5 rounded hover:bg-gray-800 shrink-0"
      >
        Vault Root
      </button>
      {parts.map((part, i) => {
        const path = parts.slice(0, i + 1).join("/");
        return (
          <span key={path} className="flex items-center gap-1 shrink-0">
            <span className="text-gray-600" aria-hidden="true">/</span>
            <button
              onClick={() => navigateTo(path)}
              className="text-gray-400 hover:text-white px-1.5 py-0.5 rounded hover:bg-gray-800"
              aria-current={i === parts.length - 1 ? "location" : undefined}
            >
              {part}
            </button>
          </span>
        );
      })}
    </nav>
  );
}
