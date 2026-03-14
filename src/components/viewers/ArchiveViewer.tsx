import { useMemo, useState } from "react";
import { useFileStore } from "../../store/fileStore";
import { useDialogStore } from "../../store/dialogStore";
import { writeFile, createDir } from "../../hooks/useTauriCommands";
import { formatFileSize } from "../../lib/fileTypes";
import { save } from "@tauri-apps/plugin-dialog";

interface ZipEntry {
  filename: string;
  compressedSize: number;
  uncompressedSize: number;
  compressionMethod: number;
  lastModified: Date;
  isDirectory: boolean;
  depth: number;
  localHeaderOffset: number;
}

const NON_ZIP_ARCHIVE_EXTS = new Set(["tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "zst"]);

function parseZip(bytes: Uint8Array): ZipEntry[] | null {
  const len = bytes.length;
  let eocdOffset = -1;
  const searchStart = Math.max(0, len - 65557);
  for (let i = len - 22; i >= searchStart; i--) {
    if (bytes[i] === 0x50 && bytes[i + 1] === 0x4b && bytes[i + 2] === 0x05 && bytes[i + 3] === 0x06) {
      eocdOffset = i;
      break;
    }
  }
  if (eocdOffset === -1) return null;

  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const totalEntries = view.getUint16(eocdOffset + 10, true);
  const cdOffset = view.getUint32(eocdOffset + 16, true);
  if (cdOffset >= len) return null;

  const entries: ZipEntry[] = [];
  let offset = cdOffset;

  for (let i = 0; i < totalEntries; i++) {
    if (offset + 46 > len) break;
    if (bytes[offset] !== 0x50 || bytes[offset + 1] !== 0x4b || bytes[offset + 2] !== 0x01 || bytes[offset + 3] !== 0x02) break;

    const compressionMethod = view.getUint16(offset + 10, true);
    const compressedSize = view.getUint32(offset + 20, true);
    const uncompressedSize = view.getUint32(offset + 24, true);
    const filenameLen = view.getUint16(offset + 28, true);
    const extraLen = view.getUint16(offset + 30, true);
    const commentLen = view.getUint16(offset + 32, true);
    const localHeaderOffset = view.getUint32(offset + 42, true);

    const dosTime = view.getUint16(offset + 12, true);
    const dosDate = view.getUint16(offset + 14, true);
    const lastModified = dosToDate(dosDate, dosTime);

    if (offset + 46 + filenameLen > len) break;
    const filename = new TextDecoder().decode(bytes.slice(offset + 46, offset + 46 + filenameLen));
    const isDirectory = filename.endsWith("/");
    const depth = filename.replace(/\/$/, "").split("/").length - 1;

    entries.push({ filename, compressedSize, uncompressedSize, compressionMethod, lastModified, isDirectory, depth, localHeaderOffset });
    offset += 46 + filenameLen + extraLen + commentLen;
  }
  return entries;
}

function getCompressedData(bytes: Uint8Array, entry: ZipEntry): Uint8Array {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const off = entry.localHeaderOffset;
  // Local file header: signature(4) + ... + filenameLen(26) + extraLen(28) + filename + extra + data
  const filenameLen = view.getUint16(off + 26, true);
  const extraLen = view.getUint16(off + 28, true);
  const dataStart = off + 30 + filenameLen + extraLen;
  return bytes.slice(dataStart, dataStart + entry.compressedSize);
}

async function decompressEntry(bytes: Uint8Array, entry: ZipEntry): Promise<Uint8Array> {
  if (entry.isDirectory) return new Uint8Array(0);
  const compressed = getCompressedData(bytes, entry);

  if (entry.compressionMethod === 0) {
    // Stored — no compression
    return compressed;
  }
  if (entry.compressionMethod === 8) {
    // Deflate — use browser DecompressionStream
    const ds = new DecompressionStream("deflate-raw");
    const writer = ds.writable.getWriter();
    const reader = ds.readable.getReader();

    // Zip bomb protection: abort if decompressed size > 100x compressed size
    const maxOutput = Math.max(compressed.length * 100, 100 * 1024 * 1024); // min 100 MB
    const writePromise = writer.write(compressed).then(() => writer.close());
    const chunks: Uint8Array[] = [];
    let totalLen = 0;

    const readPromise = (async () => {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        totalLen += value.length;
        if (totalLen > maxOutput) {
          reader.cancel();
          throw new Error("Zip bomb detected: decompression ratio too high");
        }
        chunks.push(value);
      }
    })();

    await Promise.all([writePromise, readPromise]);

    const result = new Uint8Array(totalLen);
    let pos = 0;
    for (const chunk of chunks) {
      result.set(chunk, pos);
      pos += chunk.length;
    }
    return result;
  }
  throw new Error(`Unsupported compression method: ${entry.compressionMethod}`);
}

function dosToDate(date: number, time: number): Date {
  const day = date & 0x1f;
  const month = ((date >> 5) & 0x0f) - 1;
  const year = ((date >> 9) & 0x7f) + 1980;
  const seconds = (time & 0x1f) * 2;
  const minutes = (time >> 5) & 0x3f;
  const hours = (time >> 11) & 0x1f;
  return new Date(year, month, day, hours, minutes, seconds);
}

function sortEntries(entries: ZipEntry[]): ZipEntry[] {
  return [...entries].sort((a, b) => {
    if (a.isDirectory && !b.isDirectory) return -1;
    if (!a.isDirectory && b.isDirectory) return 1;
    return a.filename.localeCompare(b.filename);
  });
}

function formatModified(d: Date): string {
  if (isNaN(d.getTime()) || d.getFullYear() < 1980) return "";
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

function displayName(filename: string): string {
  const cleaned = filename.replace(/\/$/, "");
  const parts = cleaned.split("/");
  return parts[parts.length - 1];
}

export function ArchiveViewer({ tabIndex }: { tabIndex: number }) {
  const { openTabs, currentPath, startBusy, stopBusy, refresh } = useFileStore();
  const { showConfirm } = useDialogStore();
  const tab = openTabs[tabIndex];
  const content = tab?.content;
  const [extracting, setExtracting] = useState<string | null>(null);

  const ext = tab?.name.split(".").pop()?.toLowerCase() ?? "";
  const isNonZipArchive = NON_ZIP_ARCHIVE_EXTS.has(ext);

  const { parsed, rawBytes } = useMemo(() => {
    if (!content || content.type !== "Binary" || isNonZipArchive) return { parsed: null, rawBytes: null };
    const raw = atob(content.data);
    const bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
    return { parsed: parseZip(bytes), rawBytes: bytes };
  }, [content, isNonZipArchive]);

  const showError = (msg: string) => {
    showConfirm({ title: "Error", message: msg, confirmLabel: "OK", onConfirm: () => {} });
  };

  const handleExtractToVault = async (entry: ZipEntry) => {
    if (!rawBytes || entry.isDirectory) return;
    setExtracting(entry.filename);
    try {
      const data = await decompressEntry(rawBytes, entry);
      const name = displayName(entry.filename);
      const destPath = currentPath ? `${currentPath}/${name}` : name;
      await writeFile(destPath, Array.from(data));

      refresh();
    } catch (err) {
      showError(`Extract failed: ${err}`);
    } finally {
      setExtracting(null);
    }
  };

  const handleExtractToDisk = async (entry: ZipEntry) => {
    if (!rawBytes || entry.isDirectory) return;
    const name = displayName(entry.filename);
    const dest = await save({ defaultPath: name });
    if (!dest) return;
    setExtracting(entry.filename);
    try {
      const data = await decompressEntry(rawBytes, entry);
      // Write raw bytes to disk via a temporary approach: write to vault then export
      // Actually, we can use the Tauri fs API or just use writeFile to a temp location
      // Simplest: use fetch + blob download since we have the bytes in JS
      const blob = new Blob([data]);
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = name;
      a.click();
      URL.revokeObjectURL(url);

    } catch (err) {
      showError(`Extract failed: ${err}`);
    } finally {
      setExtracting(null);
    }
  };

  const handleExtractAllToVault = async () => {
    if (!rawBytes || !parsed) return;
    const files = parsed.filter((e) => !e.isDirectory);
    if (files.length === 0) return;

    startBusy(`Extracting ${files.length} file(s)...`);
    let errors = 0;
    try {
      // Create directories first
      const dirs = parsed.filter((e) => e.isDirectory);
      for (const dir of dirs) {
        const parts = dir.filename.replace(/\/$/, "").split("/");
        const name = parts[parts.length - 1];
        const parent = parts.length > 1
          ? (currentPath ? `${currentPath}/${parts.slice(0, -1).join("/")}` : parts.slice(0, -1).join("/"))
          : currentPath;
        try {
          await createDir(parent, name);
        } catch {
          // directory may already exist
        }
      }
      // Extract files
      for (const entry of files) {
        try {
          const data = await decompressEntry(rawBytes, entry);
          const destPath = currentPath ? `${currentPath}/${entry.filename}` : entry.filename;
          await writeFile(destPath, Array.from(data));
        } catch {
          errors++;
        }
      }

      refresh();
    } catch (err) {
      showError(`Extract failed: ${err}`);
    } finally {
      stopBusy();
    }
    if (errors > 0) {
      showError(`Extracted ${files.length - errors} of ${files.length} files (${errors} failed)`);
    }
  };

  if (!content) return null;

  if (isNonZipArchive) {
    return (
      <div className="flex-1 flex items-center justify-center bg-gray-950 p-4">
        <div className="text-center">
          <svg className="w-12 h-12 text-gray-600 mx-auto mb-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8" />
          </svg>
          <p className="text-gray-400 text-sm">Archive preview is only supported for .zip files</p>
          <p className="text-gray-600 text-xs mt-1">{tab.name}</p>
        </div>
      </div>
    );
  }

  if (parsed === null) {
    return (
      <div className="flex-1 flex items-center justify-center bg-gray-950 p-4">
        <div className="text-center">
          <svg className="w-12 h-12 text-gray-600 mx-auto mb-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <p className="text-gray-400 text-sm">Not a valid archive</p>
          <p className="text-gray-600 text-xs mt-1">{tab.name}</p>
        </div>
      </div>
    );
  }

  const sorted = sortEntries(parsed);
  const fileCount = parsed.filter((e) => !e.isDirectory).length;
  const folderCount = parsed.filter((e) => e.isDirectory).length;
  const totalSize = parsed.reduce((sum, e) => sum + e.uncompressedSize, 0);

  return (
    <div className="flex-1 flex flex-col overflow-hidden bg-gray-950">
      {/* Summary header */}
      <div className="px-4 py-2.5 border-b border-gray-800 bg-gray-900/50 shrink-0">
        <div className="flex items-center gap-2">
          <svg className="w-4 h-4 text-indigo-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8" />
          </svg>
          <span className="text-sm text-gray-300 font-medium truncate">{tab.name}</span>
          <div className="flex-1" />
          <button
            onClick={handleExtractAllToVault}
            className="flex items-center gap-1.5 px-2.5 py-1 text-xs bg-indigo-600 hover:bg-indigo-500 text-white rounded transition"
            title="Extract all files into the vault"
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
            </svg>
            Extract All to Vault
          </button>
        </div>
        <p className="text-xs text-gray-500 mt-1">
          {fileCount} file{fileCount !== 1 ? "s" : ""}, {folderCount} folder{folderCount !== 1 ? "s" : ""}, {formatFileSize(totalSize)} total
        </p>
      </div>

      {/* Table */}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-xs">
          <thead className="sticky top-0 bg-gray-900 z-10">
            <tr className="text-gray-500 text-left uppercase tracking-wider">
              <th className="px-4 py-2 font-medium">Name</th>
              <th className="px-4 py-2 font-medium text-right w-24">Size</th>
              <th className="px-4 py-2 font-medium text-right w-24">Compressed</th>
              <th className="px-4 py-2 font-medium w-40">Modified</th>
              <th className="px-4 py-2 font-medium w-24 text-center">Actions</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map((entry, i) => (
              <tr key={entry.filename + i} className="border-t border-gray-800/50 hover:bg-gray-900/50">
                <td className="px-4 py-1.5">
                  <div className="flex items-center gap-1.5" style={{ paddingLeft: `${entry.depth * 16}px` }}>
                    {entry.isDirectory ? (
                      <svg className="w-3.5 h-3.5 text-indigo-400 shrink-0" fill="currentColor" viewBox="0 0 24 24">
                        <path d="M10 4H4a2 2 0 00-2 2v12a2 2 0 002 2h16a2 2 0 002-2V8a2 2 0 00-2-2h-8l-2-2z" />
                      </svg>
                    ) : (
                      <svg className="w-3.5 h-3.5 text-gray-500 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
                      </svg>
                    )}
                    <span className={entry.isDirectory ? "text-indigo-300" : "text-gray-300"}>
                      {displayName(entry.filename)}
                    </span>
                  </div>
                </td>
                <td className="px-4 py-1.5 text-right text-gray-400 font-mono">
                  {entry.isDirectory ? "" : formatFileSize(entry.uncompressedSize)}
                </td>
                <td className="px-4 py-1.5 text-right text-gray-500 font-mono">
                  {entry.isDirectory ? "" : formatFileSize(entry.compressedSize)}
                </td>
                <td className="px-4 py-1.5 text-gray-500">
                  {formatModified(entry.lastModified)}
                </td>
                <td className="px-4 py-1.5 text-center">
                  {!entry.isDirectory && (
                    <div className="flex items-center justify-center gap-1">
                      <button
                        onClick={() => handleExtractToVault(entry)}
                        disabled={extracting !== null}
                        className="p-1 rounded hover:bg-gray-700 text-gray-500 hover:text-indigo-400 disabled:opacity-30"
                        title="Extract to vault"
                      >
                        {extracting === entry.filename ? (
                          <svg className="w-3.5 h-3.5 animate-spin" fill="none" viewBox="0 0 24 24">
                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                          </svg>
                        ) : (
                          <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                          </svg>
                        )}
                      </button>
                      <button
                        onClick={() => handleExtractToDisk(entry)}
                        disabled={extracting !== null}
                        className="p-1 rounded hover:bg-gray-700 text-gray-500 hover:text-green-400 disabled:opacity-30"
                        title="Save to disk"
                      >
                        <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                        </svg>
                      </button>
                    </div>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
