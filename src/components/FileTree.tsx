import { useState, useEffect, useCallback } from "react";
import { useFileStore } from "../store/fileStore";
import { listDir } from "../hooks/useTauriCommands";

interface TreeNode {
  name: string;
  path: string;
  children: TreeNode[] | null;
  expanded: boolean;
}

export function FileTree() {
  const { currentPath, navigateTo } = useFileStore();
  const [rootChildren, setRootChildren] = useState<TreeNode[]>([]);

  const loadChildren = useCallback(async (path: string): Promise<TreeNode[]> => {
    try {
      const entries = await listDir(path);
      return entries
        .filter((e) => e.is_dir)
        .map((e) => ({
          name: e.name,
          path: path ? `${path}/${e.name}` : e.name,
          children: null,
          expanded: false,
        }));
    } catch {
      return [];
    }
  }, []);

  useEffect(() => {
    loadChildren("").then(setRootChildren);
  }, [loadChildren]);

  return (
    <div className="w-56 bg-gray-900 border-r border-gray-800 overflow-y-auto text-sm shrink-0">
      <div className="px-3 py-2 text-gray-500 text-xs uppercase tracking-wide font-medium">
        Folders
      </div>
      <button
        onClick={() => navigateTo("")}
        className={`w-full text-left px-3 py-1.5 flex items-center gap-2 ${
          currentPath === "" ? "bg-indigo-900/30 text-indigo-300" : "text-gray-400 hover:bg-gray-800 hover:text-white"
        }`}
      >
        <svg className="w-4 h-4 text-indigo-400" fill="currentColor" viewBox="0 0 24 24">
          <path d="M10 4H4a2 2 0 00-2 2v12a2 2 0 002 2h16a2 2 0 002-2V8a2 2 0 00-2-2h-8l-2-2z" />
        </svg>
        Vault Root
      </button>
      {rootChildren.map((node) => (
        <TreeNodeComponent
          key={node.path}
          node={node}
          depth={1}
          currentPath={currentPath}
          navigateTo={navigateTo}
          loadChildren={loadChildren}
        />
      ))}
    </div>
  );
}

function TreeNodeComponent({
  node,
  depth,
  currentPath,
  navigateTo,
  loadChildren,
}: {
  node: TreeNode;
  depth: number;
  currentPath: string;
  navigateTo: (path: string) => void;
  loadChildren: (path: string) => Promise<TreeNode[]>;
}) {
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<TreeNode[] | null>(null);
  const isActive = currentPath === node.path;

  const handleToggle = async () => {
    if (!expanded && children === null) {
      const loaded = await loadChildren(node.path);
      setChildren(loaded);
    }
    setExpanded(!expanded);
  };

  return (
    <div>
      <button
        className={`w-full text-left px-3 py-1 flex items-center gap-1.5 ${
          isActive ? "bg-indigo-900/30 text-indigo-300" : "text-gray-400 hover:bg-gray-800 hover:text-white"
        }`}
        style={{ paddingLeft: `${depth * 12 + 12}px` }}
        onClick={() => navigateTo(node.path)}
        onDoubleClick={handleToggle}
      >
        <button
          onClick={(e) => { e.stopPropagation(); handleToggle(); }}
          className="p-0.5 -ml-1 hover:bg-gray-700 rounded"
        >
          <svg
            className={`w-3 h-3 transition-transform ${expanded ? "rotate-90" : ""}`}
            fill="currentColor"
            viewBox="0 0 24 24"
          >
            <path d="M9 5l7 7-7 7" />
          </svg>
        </button>
        <svg className="w-4 h-4 text-indigo-400 shrink-0" fill="currentColor" viewBox="0 0 24 24">
          <path d="M10 4H4a2 2 0 00-2 2v12a2 2 0 002 2h16a2 2 0 002-2V8a2 2 0 00-2-2h-8l-2-2z" />
        </svg>
        <span className="truncate">{node.name}</span>
      </button>
      {expanded && children && children.map((child) => (
        <TreeNodeComponent
          key={child.path}
          node={child}
          depth={depth + 1}
          currentPath={currentPath}
          navigateTo={navigateTo}
          loadChildren={loadChildren}
        />
      ))}
    </div>
  );
}
