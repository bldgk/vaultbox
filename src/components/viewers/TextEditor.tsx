import { useState, useCallback, useEffect, useRef } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import { useFileStore } from "../../store/fileStore";
import { writeFile } from "../../hooks/useTauriCommands";

function getLanguage(filename: string): string {
  const ext = filename.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    js: "javascript", jsx: "javascript", ts: "typescript", tsx: "typescript",
    json: "json", md: "markdown", html: "html", css: "css", scss: "scss",
    py: "python", rs: "rust", go: "go", java: "java", c: "c", cpp: "cpp",
    h: "c", hpp: "cpp", sh: "shell", bash: "shell", zsh: "shell",
    yml: "yaml", yaml: "yaml", xml: "xml", sql: "sql", graphql: "graphql",
    toml: "ini", ini: "ini", cfg: "ini", conf: "ini",
    rb: "ruby", php: "php", swift: "swift", kt: "kotlin",
    csv: "plaintext", log: "plaintext", txt: "plaintext",
    dockerfile: "dockerfile", makefile: "plaintext",
    svg: "xml", vue: "html", svelte: "html",
  };
  return map[ext] || "plaintext";
}

export function TextEditor({ tabIndex }: { tabIndex: number }) {
  const { openTabs, markTabModified, updateTabContent } = useFileStore();
  const tab = openTabs[tabIndex];
  const content = tab?.content;
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const [initialValue, setInitialValue] = useState("");

  useEffect(() => {
    if (content?.type === "Text") {
      setInitialValue(content.data);
    }
  }, [content]);

  const handleSave = useCallback(async () => {
    if (!tab || !editorRef.current) return;
    const text = editorRef.current.getValue();
    try {
      const encoder = new TextEncoder();
      const bytes = Array.from(encoder.encode(text));
      await writeFile(tab.path, bytes);
      markTabModified(tabIndex, false);
      updateTabContent(tabIndex, { type: "Text", data: text });
    } catch (err) {
      alert(`Failed to save: ${err}`);
    }
  }, [tab, tabIndex, markTabModified, updateTabContent]);

  const handleMount: OnMount = useCallback((editor, monaco) => {
    editorRef.current = editor;
    // Bind Ctrl/Cmd+S to save
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      handleSave();
    });
  }, [handleSave]);

  const handleChange = useCallback(() => {
    markTabModified(tabIndex, true);
  }, [tabIndex, markTabModified]);

  if (!tab || !content) return null;

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-1.5 bg-gray-900 border-b border-gray-800 shrink-0">
        <span className="text-sm text-gray-400 truncate">{tab.name}</span>
        {tab.modified && <span className="text-xs text-yellow-500">(unsaved)</span>}
        <div className="flex-1" />
        <button
          onClick={handleSave}
          disabled={!tab.modified}
          className="px-2.5 py-1 bg-indigo-600 text-white text-xs rounded hover:bg-indigo-700 disabled:opacity-40 disabled:cursor-default transition"
        >
          Save
        </button>
      </div>
      <div className="flex-1 overflow-hidden">
        <Editor
          defaultValue={initialValue}
          language={getLanguage(tab.name)}
          theme="vs-dark"
          onMount={handleMount}
          onChange={handleChange}
          options={{
            fontSize: 13,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            wordWrap: "on",
            lineNumbers: "on",
            renderLineHighlight: "gutter",
            padding: { top: 8 },
            automaticLayout: true,
          }}
        />
      </div>
    </div>
  );
}
