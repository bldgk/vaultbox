# VaultBox

A desktop application for browsing gocryptfs-encrypted vaults without mounting them. All decryption happens in-process memory — no FUSE mount, no temp files on disk, no plaintext exposed to other apps.

> **Warning:** This is an experimental project. The cryptographic implementation has not been independently audited and test coverage is minimal. There may be bugs that corrupt data or compromise security. **Use at your own risk.** Do not rely on this as your only way to access important encrypted data — always keep backups.

## Features

- Open and browse gocryptfs v2 encrypted vaults
- Create new encrypted vaults
- View files inline: text (with syntax highlighting), images, video, audio, hex
- Edit and save text files back to the vault
- Full file operations: create, rename, copy, delete, import, export
- Search across decrypted filenames
- Auto-lock after configurable idle timeout
- Clipboard auto-clear for sensitive content
- Zero plaintext on disk — everything stays in memory

## Architecture

- **Frontend**: React + TypeScript + Tailwind CSS + Monaco Editor
- **Backend**: Rust (Tauri v2) handling all crypto operations
- **Crypto**: AES-256-GCM content encryption, EME filename encryption, scrypt KDF
- **Security**: Core dumps disabled, keys held in mlock'd / zeroize-guarded memory

## Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://rustup.rs/) (stable)
- [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)

## Development

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

The bundled app will be in `src-tauri/target/release/bundle/`.

## License

MIT
