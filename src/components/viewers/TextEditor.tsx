import { useCallback, useEffect, useRef } from "react";
import { EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching } from "@codemirror/language";
import { oneDark } from "@codemirror/theme-one-dark";
import { useFileStore } from "../../store/fileStore";
import { writeFile } from "../../hooks/useTauriCommands";

// Language imports — lazy loaded via dynamic import wouldn't help much,
// these are small (~5-20 KB each)
import { javascript } from "@codemirror/lang-javascript";
import { python } from "@codemirror/lang-python";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { rust } from "@codemirror/lang-rust";
import { cpp } from "@codemirror/lang-cpp";
import { java } from "@codemirror/lang-java";
import { xml } from "@codemirror/lang-xml";
import { sql } from "@codemirror/lang-sql";
import { go } from "@codemirror/lang-go";
import { php } from "@codemirror/lang-php";
import type { Extension } from "@codemirror/state";

function getLanguageExtension(filename: string): Extension[] {
  const ext = filename.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "js": case "jsx": return [javascript({ jsx: true })];
    case "ts": case "tsx": return [javascript({ jsx: true, typescript: true })];
    case "py": return [python()];
    case "html": case "htm": case "vue": case "svelte": return [html()];
    case "css": case "scss": case "less": return [css()];
    case "json": return [json()];
    case "md": case "markdown": return [markdown()];
    case "rs": return [rust()];
    case "c": case "cpp": case "h": case "hpp": case "cc": return [cpp()];
    case "java": case "kt": return [java()];
    case "xml": case "svg": case "plist": return [xml()];
    case "sql": return [sql()];
    case "go": return [go()];
    case "php": return [php()];
    default: return [];
  }
}

export function TextEditor({ tabIndex }: { tabIndex: number }) {
  const { openTabs, markTabModified, updateTabContent } = useFileStore();
  const tab = openTabs[tabIndex];
  const content = tab?.content;
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const tabRef = useRef(tab);
  tabRef.current = tab;

  const handleSave = useCallback(async () => {
    if (!tabRef.current || !viewRef.current) return;
    const text = viewRef.current.state.doc.toString();
    try {
      const encoder = new TextEncoder();
      const bytes = Array.from(encoder.encode(text));
      await writeFile(tabRef.current.path, bytes);
      markTabModified(tabIndex, false);
      updateTabContent(tabIndex, { type: "Text", data: text });
    } catch (err) {
      alert(`Failed to save: ${err}`);
    }
  }, [tabIndex, markTabModified, updateTabContent]);

  // Keep handleSave ref fresh for the keymap
  const saveRef = useRef(handleSave);
  saveRef.current = handleSave;

  useEffect(() => {
    if (!containerRef.current || content?.type !== "Text") return;

    const state = EditorState.create({
      doc: content.data,
      extensions: [
        lineNumbers(),
        highlightActiveLine(),
        drawSelection(),
        bracketMatching(),
        history(),
        syntaxHighlighting(defaultHighlightStyle),
        oneDark,
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          {
            key: "Mod-s",
            run: () => { saveRef.current(); return true; },
          },
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            markTabModified(tabIndex, true);
          }
        }),
        EditorView.lineWrapping,
        EditorView.theme({
          "&": { height: "100%", fontSize: "13px" },
          ".cm-scroller": { overflow: "auto" },
          ".cm-content": { padding: "8px 0" },
        }),
        ...getLanguageExtension(tab.name),
      ],
    });

    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [content, tab?.name]);

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
      <div ref={containerRef} className="flex-1 overflow-hidden" />
    </div>
  );
}
