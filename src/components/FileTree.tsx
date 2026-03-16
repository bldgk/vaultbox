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
  const { currentPath, navigateTo, refreshCounter } = useFileStore();
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

  // Load root on mount; on refresh, only update the list without resetting expanded state
  useEffect(() => {
    loadChildren("").then(setRootChildren);
  }, [loadChildren]);

  // On refresh, re-fetch root folder list and merge (preserve nodes that still exist)
  useEffect(() => {
    if (refreshCounter === 0) return;
    loadChildren("").then((fresh) => {
      setRootChildren((prev) => {
        const prevMap = new Map(prev.map((n) => [n.path, n]));
        return fresh.map((n) => prevMap.get(n.path) ?? n);
      });
    });
  }, [refreshCounter, loadChildren]);

  return (
    <nav className="w-56 bg-gray-900 border-r border-gray-800 overflow-y-auto text-sm shrink-0" aria-label="Folder tree">
      <div className="px-3 py-2 text-gray-500 text-xs uppercase tracking-wide font-medium" aria-hidden="true">
        Folders
      </div>
      <div role="tree" aria-label="Vault folders">
      <button
        role="treeitem"
        aria-selected={currentPath === ""}
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
    </nav>
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
    <div role="treeitem" aria-expanded={expanded} aria-selected={isActive} aria-label={node.name}>
      <button
        className={`w-full text-left px-3 py-1 flex items-center gap-1.5 ${
          isActive ? "bg-indigo-900/30 text-indigo-300" : "text-gray-400 hover:bg-gray-800 hover:text-white"
        }`}
        style={{ paddingLeft: `${depth * 12 + 12}px` }}
        onClick={() => navigateTo(node.path)}
        onDoubleClick={handleToggle}
        tabIndex={0}
      >
        <span
          role="button"
          aria-label={expanded ? "Collapse folder" : "Expand folder"}
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
        </span>
        <svg className="w-4 h-4 text-indigo-400 shrink-0" fill="currentColor" viewBox="0 0 24 24">
          <path d="M10 4H4a2 2 0 00-2 2v12a2 2 0 002 2h16a2 2 0 002-2V8a2 2 0 00-2-2h-8l-2-2z" />
        </svg>
        <span className="truncate">{node.name}</span>
      </button>
      {expanded && children && <div role="group">{children.map((child) => (
        <TreeNodeComponent
          key={child.path}
          node={child}
          depth={depth + 1}
          currentPath={currentPath}
          navigateTo={navigateTo}
          loadChildren={loadChildren}
        />
      ))}</div>}
    </div>
  );
}
