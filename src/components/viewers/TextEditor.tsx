import { useCallback, useEffect, useRef, useState } from "react";
import { EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection } from "@codemirror/view";
import { EditorState, Compartment } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching } from "@codemirror/language";
import { oneDark } from "@codemirror/theme-one-dark";
import { useFileStore } from "../../store/fileStore";
import { useDialogStore } from "../../store/dialogStore";
import { writeFile } from "../../hooks/useTauriCommands";
import type { Extension } from "@codemirror/state";

/**
 * Pressure V8's GC by allocating and discarding large ArrayBuffers.
 * This encourages the engine to reclaim unreachable string heap memory
 * (e.g. plaintext from a closed editor tab).
 */
function pressureGC() {
  try {
    for (let i = 0; i < 3; i++) {
      void new ArrayBuffer(32 * 1024 * 1024); // 32 MB
    }
  } catch {
    // Allocation failure is fine — the point is to trigger GC
  }
}

export async function loadLanguageExtension(filename: string): Promise<Extension[]> {
  const ext = filename.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "js": case "jsx": {
      const { javascript } = await import("@codemirror/lang-javascript");
      return [javascript({ jsx: true })];
    }
    case "ts": case "tsx": {
      const { javascript } = await import("@codemirror/lang-javascript");
      return [javascript({ jsx: true, typescript: true })];
    }
    case "py": {
      const { python } = await import("@codemirror/lang-python");
      return [python()];
    }
    case "html": case "htm": case "vue": case "svelte": {
      const { html } = await import("@codemirror/lang-html");
      return [html()];
    }
    case "css": case "scss": case "less": {
      const { css } = await import("@codemirror/lang-css");
      return [css()];
    }
    case "json": {
      const { json } = await import("@codemirror/lang-json");
      return [json()];
    }
    case "md": case "markdown": {
      const { markdown } = await import("@codemirror/lang-markdown");
      return [markdown()];
    }
    case "rs": {
      const { rust } = await import("@codemirror/lang-rust");
      return [rust()];
    }
    case "c": case "cpp": case "h": case "hpp": case "cc": {
      const { cpp } = await import("@codemirror/lang-cpp");
      return [cpp()];
    }
    case "java": case "kt": {
      const { java } = await import("@codemirror/lang-java");
      return [java()];
    }
    case "xml": case "svg": case "plist": {
      const { xml } = await import("@codemirror/lang-xml");
      return [xml()];
    }
    case "sql": {
      const { sql } = await import("@codemirror/lang-sql");
      return [sql()];
    }
    case "go": {
      const { go } = await import("@codemirror/lang-go");
      return [go()];
    }
    case "php": {
      const { php } = await import("@codemirror/lang-php");
      return [php()];
    }
    default: return [];
  }
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function renderMarkdown(text: string): string {
  const lines = text.split("\n");
  const htmlParts: string[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Code blocks (fenced)
    if (line.trimStart().startsWith("```")) {
      const lang = line.trimStart().slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        codeLines.push(lines[i]);
        i++;
      }
      i++; // skip closing ```
      const code = escapeHtml(codeLines.join("\n"));
      htmlParts.push(
        `<pre class="md-code-block"><code${lang ? ` class="language-${escapeHtml(lang)}"` : ""}>${code}</code></pre>`
      );
      continue;
    }

    // Blank line
    if (line.trim() === "") {
      i++;
      continue;
    }

    // Horizontal rule
    if (/^(-{3,}|_{3,}|\*{3,})\s*$/.test(line.trim())) {
      htmlParts.push('<hr class="md-hr" />');
      i++;
      continue;
    }

    // Headers
    const headerMatch = line.match(/^(#{1,6})\s+(.+)$/);
    if (headerMatch) {
      const level = headerMatch[1].length;
      htmlParts.push(`<h${level} class="md-h${level}">${renderInline(headerMatch[2])}</h${level}>`);
      i++;
      continue;
    }

    // Blockquote
    if (line.startsWith(">")) {
      const quoteLines: string[] = [];
      while (i < lines.length && (lines[i].startsWith(">") || (lines[i].trim() !== "" && quoteLines.length > 0 && !lines[i].match(/^(#{1,6}\s|```|- |\d+\. )/)))) {
        if (lines[i].startsWith(">")) {
          quoteLines.push(lines[i].replace(/^>\s?/, ""));
        } else {
          quoteLines.push(lines[i]);
        }
        i++;
      }
      htmlParts.push(`<blockquote class="md-blockquote">${renderMarkdown(quoteLines.join("\n"))}</blockquote>`);
      continue;
    }

    // Unordered list
    if (/^[-*+]\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^[-*+]\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^[-*+]\s+/, ""));
        i++;
      }
      const listItems = items.map((item) => `<li>${renderInline(item)}</li>`).join("");
      htmlParts.push(`<ul class="md-ul">${listItems}</ul>`);
      continue;
    }

    // Ordered list
    if (/^\d+\.\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\d+\.\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\d+\.\s+/, ""));
        i++;
      }
      const listItems = items.map((item) => `<li>${renderInline(item)}</li>`).join("");
      htmlParts.push(`<ol class="md-ol">${listItems}</ol>`);
      continue;
    }

    // Paragraph — collect consecutive non-blank, non-special lines
    const paraLines: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !lines[i].trimStart().startsWith("```") &&
      !lines[i].match(/^#{1,6}\s/) &&
      !lines[i].startsWith(">") &&
      !/^[-*+]\s+/.test(lines[i]) &&
      !/^\d+\.\s+/.test(lines[i]) &&
      !/^(-{3,}|_{3,}|\*{3,})\s*$/.test(lines[i].trim())
    ) {
      paraLines.push(lines[i]);
      i++;
    }
    if (paraLines.length > 0) {
      htmlParts.push(`<p class="md-p">${renderInline(paraLines.join("\n"))}</p>`);
    }
  }

  return htmlParts.join("\n");
}

function renderInline(text: string): string {
  let result = escapeHtml(text);

  // Inline code (must be before bold/italic to avoid conflicts)
  result = result.replace(/`([^`]+)`/g, '<code class="md-inline-code">$1</code>');

  // Bold
  result = result.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
  result = result.replace(/__(.+?)__/g, "<strong>$1</strong>");

  // Italic
  result = result.replace(/\*(.+?)\*/g, "<em>$1</em>");
  result = result.replace(/_(.+?)_/g, "<em>$1</em>");

  // Links — only allow http/https/mailto, block javascript: and other dangerous protocols
  result = result.replace(
    /\[([^\]]+)\]\(([^)]+)\)/g,
    (_, text, url) => {
      const trimmed = url.trim().toLowerCase();
      if (trimmed.startsWith("http://") || trimmed.startsWith("https://") || trimmed.startsWith("mailto:")) {
        return `<a class="md-link" href="${url}" target="_blank" rel="noopener noreferrer">${text}</a>`;
      }
      return text;
    }
  );

  // Line breaks within paragraphs
  result = result.replace(/\n/g, "<br />");

  return result;
}

export function TextEditor({ tabIndex }: { tabIndex: number }) {
  const { openTabs, markTabModified, updateTabContent } = useFileStore();
  const tab = openTabs[tabIndex];
  const content = tab?.content;
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const tabRef = useRef(tab);
  tabRef.current = tab;

  const isMarkdown = /\.(md|markdown)$/i.test(tab?.name ?? "");
  const [showPreview, setShowPreview] = useState(false);
  const [previewHtml, setPreviewHtml] = useState("");

  // Reset preview when switching to a non-markdown file
  useEffect(() => {
    if (!isMarkdown && showPreview) {
      setShowPreview(false);
      setPreviewHtml("");
    }
  }, [isMarkdown]);

  const handleSave = useCallback(async () => {
    if (!tabRef.current || !viewRef.current) return;
    const { startBusy, stopBusy } = useFileStore.getState();
    startBusy(`Saving ${tabRef.current.name}...`);
    const text = viewRef.current.state.doc.toString();
    try {
      const encoder = new TextEncoder();
      const bytes = Array.from(encoder.encode(text));
      await writeFile(tabRef.current.path, bytes);
      markTabModified(tabIndex, false);
      updateTabContent(tabIndex, { type: "Text", data: text });
    } catch (err) {
      useDialogStore.getState().showConfirm({
        title: "Error",
        message: `Failed to save: ${err}`,
        confirmLabel: "OK",
        onConfirm: () => {},
      });
    } finally {
      stopBusy();
    }
  }, [tabIndex, markTabModified, updateTabContent]);

  // Keep handleSave ref fresh for the keymap
  const saveRef = useRef(handleSave);
  saveRef.current = handleSave;

  // Ref for preview HTML updater so the EditorView listener can call it without stale closures
  const previewHtmlRef = useRef(setPreviewHtml);
  previewHtmlRef.current = setPreviewHtml;
  const isMarkdownRef = useRef(isMarkdown);
  isMarkdownRef.current = isMarkdown;

  useEffect(() => {
    if (!containerRef.current || content?.type !== "Text") return;

    // Initialize preview HTML if this is a markdown file
    if (isMarkdownRef.current) {
      previewHtmlRef.current(renderMarkdown(content.data));
    }

    // Compartment allows reconfiguring the language extension after async load
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
            markTabModified(tabIndex, true);
            // Update preview HTML live as the user types
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

    // Load language extension asynchronously, then inject via compartment
    let cancelled = false;
    loadLanguageExtension(tab.name).then((langExt) => {
      if (!cancelled && langExt.length > 0) {
        view.dispatch({ effects: langCompartment.reconfigure(langExt) });
      }
    });

    return () => {
      cancelled = true;
      // Security: overwrite document content before destroying to help V8 release plaintext strings
      try {
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: "" },
        });
      } catch {
        // View may already be detached
      }
      view.destroy();
      viewRef.current = null;
      // Pressure GC to reclaim string heap — allocate and discard large ArrayBuffers
      pressureGC();
    };
  }, [content, tab?.name]);

  // When toggling preview on, regenerate from current editor content
  useEffect(() => {
    if (showPreview && isMarkdown && viewRef.current) {
      const docText = viewRef.current.state.doc.toString();
      setPreviewHtml(renderMarkdown(docText));
    }
  }, [showPreview, isMarkdown]);

  if (!tab || !content) return null;

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-1.5 bg-gray-900 border-b border-gray-800 shrink-0">
        <span className="text-sm text-gray-400 truncate">{tab.name}</span>
        {tab.modified && <span className="text-xs text-yellow-500">(unsaved)</span>}
        <div className="flex-1" />
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
                /* Editor-only icon (code brackets) */
                <path
                  fillRule="evenodd"
                  d="M6.28 5.22a.75.75 0 010 1.06L2.56 10l3.72 3.72a.75.75 0 01-1.06 1.06L.97 10.53a.75.75 0 010-1.06l4.25-4.25a.75.75 0 011.06 0zm7.44 0a.75.75 0 011.06 0l4.25 4.25a.75.75 0 010 1.06l-4.25 4.25a.75.75 0 01-1.06-1.06L17.44 10l-3.72-3.72a.75.75 0 010-1.06z"
                  clipRule="evenodd"
                />
              ) : (
                /* Split-screen icon (two columns) */
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
          disabled={!tab.modified}
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
