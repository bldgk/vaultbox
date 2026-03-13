import { useMemo } from "react";
import { useFileStore } from "../../store/fileStore";

export function HexViewer({ tabIndex }: { tabIndex: number }) {
  const { openTabs } = useFileStore();
  const tab = openTabs[tabIndex];
  const content = tab?.content;

  const lines = useMemo(() => {
    if (!content || content.type !== "Binary") return [];
    const raw = atob(content.data);
    const bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);

    const result: { offset: string; hex: string; ascii: string }[] = [];
    const limit = Math.min(bytes.length, 16 * 256); // Show first 4KB

    for (let i = 0; i < limit; i += 16) {
      const chunk = bytes.slice(i, Math.min(i + 16, limit));
      const offset = i.toString(16).padStart(8, "0");
      const hex = Array.from(chunk)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join(" ");
      const ascii = Array.from(chunk)
        .map((b) => (b >= 32 && b < 127 ? String.fromCharCode(b) : "."))
        .join("");
      result.push({ offset, hex: hex.padEnd(47, " "), ascii });
    }
    return result;
  }, [content]);

  if (!content) return null;

  return (
    <div className="flex-1 overflow-auto p-4 bg-gray-950 font-mono text-xs">
      <div className="text-gray-500 mb-2">{tab.name} (read-only)</div>
      {lines.map((line) => (
        <div key={line.offset} className="flex gap-4">
          <span className="text-indigo-400">{line.offset}</span>
          <span className="text-gray-400">{line.hex}</span>
          <span className="text-gray-600">{line.ascii}</span>
        </div>
      ))}
      {content.type === "Binary" && atob(content.data).length > 16 * 256 && (
        <div className="text-gray-600 mt-2">... (truncated)</div>
      )}
    </div>
  );
}
