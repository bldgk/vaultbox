import { invoke } from "@tauri-apps/api/core";

export interface FileEntry {
  name: string;
  is_dir: boolean;
  size: number;
  modified: number;
  encrypted_name: string;
}

export interface VaultInfo {
  path: string;
  version: number;
  feature_flags: string[];
}

export interface VaultStatusResponse {
  status: "locked" | "unlocked";
  path: string | null;
}

export type FileContent =
  | { type: "Text"; data: string }
  | { type: "Binary"; data: string };

export async function openVault(path: string, password: string, configPath?: string): Promise<VaultInfo> {
  return invoke<VaultInfo>("open_vault", { path, password, configPath: configPath ?? null });
}

export async function createVault(path: string, password: string): Promise<VaultInfo> {
  return invoke<VaultInfo>("create_vault", { path, password });
}

export async function lockVault(): Promise<void> {
  return invoke("lock_vault");
}

export async function getVaultStatus(): Promise<VaultStatusResponse> {
  return invoke<VaultStatusResponse>("get_vault_status");
}

export async function listDir(path: string): Promise<FileEntry[]> {
  return invoke<FileEntry[]>("list_dir", { path });
}

export async function readFile(path: string): Promise<FileContent> {
  return invoke<FileContent>("read_file", { path });
}

export async function writeFile(path: string, content: number[]): Promise<void> {
  return invoke("write_file", { path, content });
}

export async function createFile(dir: string, name: string): Promise<void> {
  return invoke("create_file", { dir, name });
}

export async function createDir(parent: string, name: string): Promise<void> {
  return invoke("create_dir", { parent, name });
}

export async function renameEntry(oldPath: string, newName: string): Promise<void> {
  return invoke("rename_entry", { oldPath, newName });
}

export async function deleteEntry(path: string, permanent: boolean): Promise<void> {
  return invoke("delete_entry", { path, permanent });
}

export async function copyEntry(sourcePath: string, destDir: string, destName: string): Promise<void> {
  return invoke("copy_entry", { sourcePath, destDir, destName });
}

export async function searchFiles(query: string): Promise<FileEntry[]> {
  return invoke<FileEntry[]>("search_files", { query });
}

export async function importFiles(externalPaths: string[], vaultDir: string): Promise<void> {
  return invoke("import_files", { externalPaths, vaultDir });
}

export async function exportFile(vaultPath: string, externalDest: string): Promise<void> {
  return invoke("export_file", { vaultPathStr: vaultPath, externalDest });
}
