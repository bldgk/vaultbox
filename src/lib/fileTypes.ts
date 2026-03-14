export type ViewerType = "text" | "image" | "pdf" | "media" | "hex" | "archive";

const TEXT_EXTENSIONS = new Set([
  "txt", "md", "json", "yml", "yaml", "csv", "log", "xml", "html", "css",
  "js", "ts", "jsx", "tsx", "py", "rs", "go", "java", "c", "cpp", "h",
  "hpp", "sh", "bash", "zsh", "fish", "toml", "ini", "cfg", "conf",
  "env", "gitignore", "dockerfile", "makefile", "sql", "graphql", "vue",
  "svelte", "astro", "rb", "php", "swift", "kt", "scala", "r",
]);

const IMAGE_EXTENSIONS = new Set([
  "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif",
]);

const PDF_EXTENSIONS = new Set(["pdf"]);

const MEDIA_EXTENSIONS = new Set([
  "mp4", "m4v", "mov", "mp3", "wav", "ogg", "ogv", "webm", "m4a", "flac",
  "aac", "avi", "mkv", "3gp", "ts",
]);

const ARCHIVE_EXTENSIONS = new Set(["zip"]);

export function getViewerType(filename: string): ViewerType {
  const ext = filename.split(".").pop()?.toLowerCase() ?? "";
  if (TEXT_EXTENSIONS.has(ext)) return "text";
  if (IMAGE_EXTENSIONS.has(ext)) return "image";
  if (PDF_EXTENSIONS.has(ext)) return "pdf";
  if (MEDIA_EXTENSIONS.has(ext)) return "media";
  if (ARCHIVE_EXTENSIONS.has(ext)) return "archive";
  return "hex";
}

export function getFileIcon(filename: string, isDir: boolean): string {
  if (isDir) return "folder";
  const ext = filename.split(".").pop()?.toLowerCase() ?? "";
  if (TEXT_EXTENSIONS.has(ext)) return "file-text";
  if (IMAGE_EXTENSIONS.has(ext)) return "file-image";
  if (PDF_EXTENSIONS.has(ext)) return "file-pdf";
  if (MEDIA_EXTENSIONS.has(ext)) return "file-media";
  return "file";
}

export function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

export function formatDate(timestamp: number): string {
  if (timestamp === 0) return "";
  return new Date(timestamp * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
