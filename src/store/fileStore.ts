import { create } from "zustand";
import type { FileEntry, FileContent } from "../hooks/useTauriCommands";

/**
 * Pressure V8's GC to reclaim unreachable string heap memory
 * after closing tabs that held decrypted plaintext.
 */
function pressureGC() {
  try {
    for (let i = 0; i < 3; i++) {
      void new ArrayBuffer(32 * 1024 * 1024); // 32 MB
    }
  } catch {
    // Allocation failure is fine
  }
}

export interface OpenTab {
  path: string;
  name: string;
  content: FileContent | null;
  modified: boolean;
}

export interface ClipboardState {
  files: string[]; // full paths
  names: string[]; // just filenames
  sourceDir: string;
  operation: "copy" | "cut";
}

export interface FullscreenPreview {
  filePath: string;
  fileName: string;
  content: FileContent;
}

interface FileStore {
  currentPath: string;
  refreshCounter: number;
  entries: FileEntry[];
  selectedFiles: Set<string>;
  openTabs: OpenTab[];
  activeTabIndex: number;
  viewMode: "grid" | "list";
  sortBy: "name" | "size" | "modified";
  sortAsc: boolean;
  navigationHistory: string[];
  historyIndex: number;
  searchQuery: string;
  searchResults: FileEntry[] | null;
  loading: boolean;
  busyCount: number;
  statusText: string;
  clipboard: ClipboardState | null;
  fullscreenPreview: FullscreenPreview | null;
  showInfoPanel: boolean;
  fileTreeCollapsed: boolean;
  fileListCollapsed: boolean;
  splitView: boolean;
  splitTabIndex: number;

  setCurrentPath: (path: string) => void;
  refresh: () => void;
  setEntries: (entries: FileEntry[]) => void;
  setSelectedFiles: (files: Set<string>) => void;
  toggleSelection: (name: string, multi: boolean) => void;
  openTab: (tab: OpenTab) => void;
  closeTab: (index: number) => void;
  setActiveTab: (index: number) => void;
  updateTabContent: (index: number, content: FileContent) => void;
  markTabModified: (index: number, modified: boolean) => void;
  setViewMode: (mode: "grid" | "list") => void;
  setSortBy: (sort: "name" | "size" | "modified") => void;
  navigateTo: (path: string) => void;
  goBack: () => void;
  goForward: () => void;
  setSearchQuery: (query: string) => void;
  setSearchResults: (results: FileEntry[] | null) => void;
  setLoading: (loading: boolean) => void;
  startBusy: (status?: string) => void;
  stopBusy: () => void;
  setStatusText: (text: string) => void;
  setClipboard: (clipboard: ClipboardState | null) => void;
  reorderTab: (fromIndex: number, toIndex: number) => void;
  setFullscreenPreview: (preview: FullscreenPreview | null) => void;
  toggleInfoPanel: () => void;
  toggleFileTree: () => void;
  toggleFileList: () => void;
  toggleSplitView: () => void;
  setSplitTab: (index: number) => void;
  reset: () => void;
}

export const useFileStore = create<FileStore>((set) => ({
  currentPath: "",
  refreshCounter: 0,
  entries: [],
  selectedFiles: new Set(),
  openTabs: [],
  activeTabIndex: -1,
  viewMode: "list",
  sortBy: "name",
  sortAsc: true,
  navigationHistory: [""],
  historyIndex: 0,
  searchQuery: "",
  searchResults: null,
  loading: false,
  busyCount: 0,
  statusText: "",
  clipboard: null,
  fullscreenPreview: null,
  showInfoPanel: false,
  fileTreeCollapsed: false,
  fileListCollapsed: false,
  splitView: false,
  splitTabIndex: -1,

  setCurrentPath: (path) => set({ currentPath: path }),
  refresh: () => set((state) => ({ refreshCounter: state.refreshCounter + 1 })),
  setEntries: (entries) => set({ entries }),
  setSelectedFiles: (files) => set({ selectedFiles: files }),

  toggleSelection: (name, multi) =>
    set((state) => {
      const newSet = new Set(multi ? state.selectedFiles : []);
      if (newSet.has(name)) {
        newSet.delete(name);
      } else {
        newSet.add(name);
      }
      return { selectedFiles: newSet };
    }),

  openTab: (tab) =>
    set((state) => {
      const existing = state.openTabs.findIndex((t) => t.path === tab.path);
      if (existing >= 0) {
        return { activeTabIndex: existing };
      }
      return {
        openTabs: [...state.openTabs, tab],
        activeTabIndex: state.openTabs.length,
      };
    }),

  closeTab: (index) =>
    set((state) => {
      // Null out the content of the closed tab to release decrypted data
      const closedTab = state.openTabs[index];
      if (closedTab) closedTab.content = null;
      const tabs = state.openTabs.filter((_, i) => i !== index);
      let activeIndex = state.activeTabIndex;
      if (activeIndex >= tabs.length) activeIndex = tabs.length - 1;
      let splitIndex = state.splitTabIndex;
      if (splitIndex === index) splitIndex = -1;
      else if (splitIndex > index) splitIndex--;
      if (splitIndex >= tabs.length) splitIndex = -1;
      // Pressure GC to reclaim decrypted plaintext from the closed tab
      setTimeout(pressureGC, 0);
      return { openTabs: tabs, activeTabIndex: activeIndex, splitTabIndex: splitIndex };
    }),

  setActiveTab: (index) => set({ activeTabIndex: index }),

  updateTabContent: (index, content) =>
    set((state) => {
      const tabs = [...state.openTabs];
      if (tabs[index]) {
        tabs[index] = { ...tabs[index], content };
      }
      return { openTabs: tabs };
    }),

  markTabModified: (index, modified) =>
    set((state) => {
      const tabs = [...state.openTabs];
      if (tabs[index]) {
        tabs[index] = { ...tabs[index], modified };
      }
      return { openTabs: tabs };
    }),

  setViewMode: (mode) => set({ viewMode: mode }),

  setSortBy: (sort) =>
    set((state) => ({
      sortBy: sort,
      sortAsc: state.sortBy === sort ? !state.sortAsc : true,
    })),

  navigateTo: (path) =>
    set((state) => {
      const history = state.navigationHistory.slice(0, state.historyIndex + 1);
      history.push(path);
      return {
        currentPath: path,
        navigationHistory: history,
        historyIndex: history.length - 1,
        selectedFiles: new Set(),
        searchResults: null,
        searchQuery: "",
      };
    }),

  goBack: () =>
    set((state) => {
      if (state.historyIndex <= 0) return state;
      const newIndex = state.historyIndex - 1;
      return {
        historyIndex: newIndex,
        currentPath: state.navigationHistory[newIndex],
        selectedFiles: new Set(),
      };
    }),

  goForward: () =>
    set((state) => {
      if (state.historyIndex >= state.navigationHistory.length - 1) return state;
      const newIndex = state.historyIndex + 1;
      return {
        historyIndex: newIndex,
        currentPath: state.navigationHistory[newIndex],
        selectedFiles: new Set(),
      };
    }),

  setSearchQuery: (query) => set({ searchQuery: query }),
  setSearchResults: (results) => set({ searchResults: results }),
  setLoading: (loading) => set({ loading }),
  startBusy: (status) => set((state) => ({ busyCount: state.busyCount + 1, statusText: status || "Working..." })),
  stopBusy: () => set((state) => ({ busyCount: Math.max(0, state.busyCount - 1), statusText: state.busyCount <= 1 ? "" : state.statusText })),
  setStatusText: (text) => set({ statusText: text }),
  reorderTab: (fromIndex, toIndex) =>
    set((state) => {
      const tabs = [...state.openTabs];
      const [moved] = tabs.splice(fromIndex, 1);
      tabs.splice(toIndex, 0, moved);
      let activeIndex = state.activeTabIndex;
      if (activeIndex === fromIndex) activeIndex = toIndex;
      else if (fromIndex < activeIndex && toIndex >= activeIndex) activeIndex--;
      else if (fromIndex > activeIndex && toIndex <= activeIndex) activeIndex++;
      return { openTabs: tabs, activeTabIndex: activeIndex };
    }),
  setClipboard: (clipboard) => set({ clipboard }),
  setFullscreenPreview: (preview) => set({ fullscreenPreview: preview }),
  toggleInfoPanel: () => set((state) => ({ showInfoPanel: !state.showInfoPanel })),
  toggleFileTree: () => set((state) => ({ fileTreeCollapsed: !state.fileTreeCollapsed })),
  toggleFileList: () => set((state) => ({ fileListCollapsed: !state.fileListCollapsed })),
  toggleSplitView: () =>
    set((state) => ({
      splitView: !state.splitView,
      splitTabIndex: state.splitView ? -1 : state.splitTabIndex,
    })),
  setSplitTab: (index) => set({ splitTabIndex: index }),

  reset: () => {
    // Null out all tab contents before resetting to release decrypted data
    const { openTabs } = useFileStore.getState();
    for (const tab of openTabs) {
      tab.content = null;
    }
    set({
      currentPath: "",
      refreshCounter: 0,
      entries: [],
      selectedFiles: new Set(),
      openTabs: [],
      activeTabIndex: -1,
      navigationHistory: [""],
      historyIndex: 0,
      searchQuery: "",
      searchResults: null,
      loading: false,
      busyCount: 0,
      statusText: "",
      clipboard: null,
      fullscreenPreview: null,
      showInfoPanel: false,
      fileTreeCollapsed: false,
      fileListCollapsed: false,
      splitView: false,
      splitTabIndex: -1,
    });
    // Pressure GC to reclaim all decrypted plaintext
    setTimeout(pressureGC, 0);
  },
}));
