import { useCallback, useEffect, useRef, useState } from "react";
import { EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection } from "@codemirror/view";
import { EditorState, Compartment } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching } from "@codemirror/language";
import { oneDark } from "@codemirror/theme-one-dark";
import { writeFile } from "../../hooks/useTauriCommands";
import type { FileContent } from "../../hooks/useTauriCommands";
import { loadLanguageExtension, renderMarkdown } from "./TextEditor";

interface Props {
  path: string;
  name: string;
  content: FileContent;
  onModified: (modified: boolean) => void;
  onSaved: (content: FileContent) => void;
}

export function IsolatedTextEditor({ path, name, content, onModified, onSaved }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const [modified, setModified] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isMarkdown = /\.(md|markdown)$/i.test(name);
  const [showPreview, setShowPreview] = useState(false);
  const [previewHtml, setPreviewHtml] = useState("");

  // Refs to avoid stale closures
  const pathRef = useRef(path);
  pathRef.current = path;
  const nameRef = useRef(name);
  nameRef.current = name;
  const onModifiedRef = useRef(onModified);
  onModifiedRef.current = onModified;
  const onSavedRef = useRef(onSaved);
  onSavedRef.current = onSaved;
  const previewHtmlRef = useRef(setPreviewHtml);
  previewHtmlRef.current = setPreviewHtml;
  const isMarkdownRef = useRef(isMarkdown);
  isMarkdownRef.current = isMarkdown;

  useEffect(() => {
    if (!isMarkdown && showPreview) {
      setShowPreview(false);
      setPreviewHtml("");
    }
  }, [isMarkdown]);

  const handleSave = useCallback(async () => {
    if (!viewRef.current) return;
    const text = viewRef.current.state.doc.toString();
    try {
      const encoder = new TextEncoder();
      const bytes = Array.from(encoder.encode(text));
      await writeFile(pathRef.current, bytes);
      setModified(false);
      onModifiedRef.current(false);
      onSavedRef.current({ type: "Text", data: text });
    } catch (err) {
      setError(`Failed to save: ${err}`);
    }
  }, []);

  const saveRef = useRef(handleSave);
  saveRef.current = handleSave;

  useEffect(() => {
    if (!containerRef.current || content?.type !== "Text") return;

    if (isMarkdownRef.current) {
      previewHtmlRef.current(renderMarkdown(content.data));
    }

    const langCompartment = new Compartment();

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
            setModified(true);
            onModifiedRef.current(true);
            if (isMarkdownRef.current) {
              const docText = update.state.doc.toString();
              previewHtmlRef.current(renderMarkdown(docText));
            }
          }
        }),
        EditorView.lineWrapping,
        EditorView.theme({
          "&": { height: "100%", fontSize: "13px" },
          ".cm-scroller": { overflow: "auto" },
          ".cm-content": { padding: "8px 0" },
        }),
        langCompartment.of([]),
      ],
    });

    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;

    let cancelled = false;
    loadLanguageExtension(name).then((langExt) => {
      if (!cancelled && langExt.length > 0) {
        view.dispatch({ effects: langCompartment.reconfigure(langExt) });
      }
    });

    return () => {
      cancelled = true;
      // No GC pressure needed here — the entire webview will be destroyed on tab close
      view.destroy();
      viewRef.current = null;
    };
  }, [content, name]);

  useEffect(() => {
    if (showPreview && isMarkdown && viewRef.current) {
      const docText = viewRef.current.state.doc.toString();
      setPreviewHtml(renderMarkdown(docText));
    }
  }, [showPreview, isMarkdown]);

  if (content?.type !== "Text") return null;

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-1.5 bg-gray-900 border-b border-gray-800 shrink-0">
        <span className="text-sm text-gray-400 truncate">{name}</span>
        {modified && <span className="text-xs text-yellow-500">(unsaved)</span>}
        <div className="flex-1" />
        {error && <span className="text-xs text-red-400 truncate max-w-[200px]">{error}</span>}
        {isMarkdown && (
          <button
            onClick={() => setShowPreview((v) => !v)}
            className="px-2.5 py-1 bg-gray-700 text-gray-200 text-xs rounded hover:bg-gray-600 transition flex items-center gap-1.5"
            title={showPreview ? "Hide preview" : "Show preview"}
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              viewBox="0 0 20 20"
              fill="currentColor"
              className="w-3.5 h-3.5"
            >
              {showPreview ? (
                <path
                  fillRule="evenodd"
                  d="M6.28 5.22a.75.75 0 010 1.06L2.56 10l3.72 3.72a.75.75 0 01-1.06 1.06L.97 10.53a.75.75 0 010-1.06l4.25-4.25a.75.75 0 011.06 0zm7.44 0a.75.75 0 011.06 0l4.25 4.25a.75.75 0 010 1.06l-4.25 4.25a.75.75 0 01-1.06-1.06L17.44 10l-3.72-3.72a.75.75 0 010-1.06z"
                  clipRule="evenodd"
                />
              ) : (
                <path
                  fillRule="evenodd"
                  d="M2 4.25A2.25 2.25 0 014.25 2h11.5A2.25 2.25 0 0118 4.25v11.5A2.25 2.25 0 0115.75 18H4.25A2.25 2.25 0 012 15.75V4.25zM4.25 3.5a.75.75 0 00-.75.75v11.5c0 .414.336.75.75.75H9.5V3.5H4.25zm7.25 0v13h4.25a.75.75 0 00.75-.75V4.25a.75.75 0 00-.75-.75H11.5z"
                  clipRule="evenodd"
                />
              )}
            </svg>
            {showPreview ? "Editor" : "Preview"}
          </button>
        )}
        <button
          onClick={handleSave}
          disabled={!modified}
          className="px-2.5 py-1 bg-indigo-600 text-white text-xs rounded hover:bg-indigo-700 disabled:opacity-40 disabled:cursor-default transition"
        >
          Save
        </button>
      </div>
      <div className="flex flex-1 overflow-hidden min-h-0">
        <div
          ref={containerRef}
          className={`overflow-hidden ${showPreview ? "w-1/2 border-r border-gray-800" : "w-full"}`}
        />
        {showPreview && (
          <div className="w-1/2 overflow-auto p-6 bg-gray-950 md-preview">
            <div
              className="md-preview-content"
              dangerouslySetInnerHTML={{ __html: previewHtml }}
            />
          </div>
        )}
      </div>
    </div>
  );
}
