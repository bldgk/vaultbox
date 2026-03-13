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
  master_key_hex?: string;
}

export interface VaultStatusResponse {
  status: "locked" | "unlocked";
  path: string | null;
}

export type FileContent =
  | { type: "Text"; data: string }
  | { type: "Binary"; data: string };

export async function openVault(path: string, password: Uint8Array, configPath?: string): Promise<VaultInfo> {
  // Send password as byte array so it can be zeroed after the call.
  // JavaScript strings are immutable and cannot be wiped from memory;
  // Uint8Array can be filled with zeros immediately after use.
  const passwordBytes = Array.from(password);
  password.fill(0);
  try {
    return await invoke<VaultInfo>("open_vault", { path, password: passwordBytes, configPath: configPath ?? null });
  } finally {
    passwordBytes.fill(0);
  }
}

export async function createVault(path: string, password: Uint8Array): Promise<VaultInfo> {
  const passwordBytes = Array.from(password);
  password.fill(0);
  try {
    return await invoke<VaultInfo>("create_vault", { path, password: passwordBytes });
  } finally {
    passwordBytes.fill(0);
  }
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
