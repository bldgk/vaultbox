import { useEffect, useState, useCallback, lazy, Suspense } from "react";
import { listen, emit } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { readFile } from "./hooks/useTauriCommands";
import type { FileContent } from "./hooks/useTauriCommands";

const IsolatedTextEditor = lazy(() =>
  import("./components/viewers/IsolatedTextEditor").then((m) => ({
    default: m.IsolatedTextEditor,
  }))
);

interface ViewerFile {
  path: string;
  name: string;
}

export function ViewerApp() {
  const [file, setFile] = useState<ViewerFile | null>(null);
  const [content, setContent] = useState<FileContent | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Get this webview's label for targeted events
  const label = getCurrentWebview().label;

  // Listen for file load commands from the main webview
  useEffect(() => {
    const unlistenLoad = listen<ViewerFile>(`viewer:load:${label}`, async (event) => {
      const { path, name } = event.payload;
      setFile({ path, name });
      setError(null);

      try {
        const fileContent = await readFile(path);
        setContent(fileContent);
      } catch (err) {
        setError(`Failed to load file: ${err}`);
        setContent(null);
      }
    });

    // Notify the main webview that this viewer is ready
    emit(`viewer:ready:${label}`, {});

    return () => {
      unlistenLoad.then((fn) => fn());
    };
  }, [label]);

  const handleModified = useCallback(
    (modified: boolean) => {
      emit(`viewer:modified:${label}`, { modified });
    },
    [label]
  );

  const handleSaved = useCallback(
    (content: FileContent) => {
      emit(`viewer:saved:${label}`, { content });
    },
    [label]
  );

  if (error) {
    return (
      <div className="flex items-center justify-center h-screen bg-gray-950 text-red-400 text-sm p-4">
        {error}
      </div>
    );
  }

  if (!file || !content) {
    return (
      <div className="flex items-center justify-center h-screen bg-gray-950 text-gray-500 text-sm">
        Loading...
      </div>
    );
  }

  return (
    <div className="h-screen bg-gray-950 overflow-hidden">
      <Suspense
        fallback={
          <div className="flex items-center justify-center h-full text-gray-500 text-sm">
            Loading editor...
          </div>
        }
      >
        <IsolatedTextEditor
          path={file.path}
          name={file.name}
          content={content}
          onModified={handleModified}
          onSaved={handleSaved}
        />
      </Suspense>
    </div>
  );
}
