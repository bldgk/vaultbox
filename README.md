# VaultBox

A desktop application for browsing gocryptfs-encrypted vaults without mounting them. All decryption happens in-process memory — no FUSE mount, no temp files on disk, no plaintext exposed to other apps.

> **Warning:** This is an experimental project. The cryptographic implementation has not been independently audited. There may be bugs that corrupt data or compromise security. **Use at your own risk.** Do not rely on this as your only way to access important encrypted data — always keep backups.

## Features

- Open and browse gocryptfs v2 encrypted vaults
- Create new encrypted vaults with master key backup
- View files inline: text (with syntax highlighting), images, video, audio, hex
- Edit and save text files back to the vault
- Full file operations: create, rename, copy, delete, import, export
- Search across decrypted filenames
- Auto-lock after 10 minutes of inactivity
- Zero plaintext on disk — everything stays in memory

## Security Model

VaultBox is designed to keep decrypted data isolated to a single process, unlike gocryptfs FUSE mounts which expose plaintext to all processes via the filesystem.

### Comparison with gocryptfs mount

| | gocryptfs FUSE mount | VaultBox |
|---|---|---|
| Plaintext access | Any process can `open("/mnt/vault/file")` | Only VaultBox process has decrypted data |
| Malware with user privileges | Reads vault files like normal files | macOS/Linux: blocked by OS; Windows: needs `ReadProcessMemory` |
| Kernel page cache | Decrypted pages cached by kernel | No kernel cache — decryption in userspace only |
| Visibility | Mount point visible via `mount`, `df` | No mount point, no filesystem exposure |
| Use from other apps | Any app (Word, Photoshop, etc.) | Built-in viewers only; export required for external apps |

### Memory protection

| Protection | How | Platform |
|------------|-----|----------|
| XOR-masked keys | Keys stored as `masked = key ⊕ random_mask`; unmasked only during crypto operations (microseconds), then re-masked with fresh random pad | All |
| mlock | All key material and plaintext cache entries pinned in RAM via `mlock()` / `VirtualLock()` — never swapped to disk | All |
| Zeroize on drop | Keys, passwords, decrypted buffers zeroed via volatile writes (`zeroize` crate) — compiler cannot optimize away | All |
| Core dumps disabled | `setrlimit(RLIMIT_CORE, 0)` at startup | Unix |
| Anti-ptrace | `prctl(PR_SET_DUMPABLE, 0)` — blocks `/proc/pid/mem` reads and `ptrace` attach from same-user processes | Linux |
| App Sandbox | macOS entitlements: no network access (`com.apple.security.network.client` absent), filesystem limited to user-selected files only | macOS |

### Password handling

| Layer | Type | Zeroed after use? |
|-------|------|-------------------|
| `<input>` DOM | Browser-managed (not controllable) | No (WebView limitation) |
| `useSecurePassword` hook | `number[]` (mutable ref, not a JS string) | Yes — `fill(0)` on consume |
| IPC transport | `Uint8Array` → `number[]` | Yes — both zeroed in `finally` |
| Rust command handler | `Vec<u8>` → `String` for scrypt | Yes — both `.zeroize()` after KDF |

### Network isolation

| Layer | Mechanism |
|-------|-----------|
| CSP `connect-src` | `'self' ipc: http://ipc.localhost vaultmedia:` — blocks all external HTTP/WebSocket from WebView |
| macOS App Sandbox | Network entitlement absent — OS blocks any TCP/UDP from the process |
| No plugins with network access | `opener` plugin removed; only `dialog` plugin remains (file open/save dialogs) |
| Rust code | No `reqwest`, `hyper`, or any HTTP client in dependencies |

### Filesystem restrictions

| Mechanism | What it does |
|-----------|-------------|
| Tauri capabilities | Minimal: `core:default` + `dialog:allow-open` + `dialog:allow-save` only |
| `validate_external_path()` | Import/export blocked for `/etc`, `/System`, `/usr`, `/bin`, `/proc`, `/sys`, all hidden directories (`.ssh`, `.gnupg`, `.aws`, `.config`, etc.) |
| macOS App Sandbox | `files.user-selected.read-write` — can only access files explicitly chosen by user through system dialogs |
| Vault operations | Path resolution constrained to `vault_path` — no traversal outside vault directory |

### Known limitations

- WebView (JavaScriptCore/Chromium) holds internal copies of DOM input values and rendered content — not controllable from application code
- JavaScript strings are immutable and GC'd, not zeroized — decrypted text in open editor tabs persists in JS heap until GC collects
- On Windows, `ReadProcessMemory` from a same-user process can read VaultBox memory (no OS-level equivalent of `PR_SET_DUMPABLE`); XOR masking makes this harder but not impossible for a targeted attacker
- Monaco/CodeMirror editor maintains internal copies of document content in JS heap

## Architecture

- **Frontend**: React 19 + TypeScript + Tailwind CSS 4 + CodeMirror 6
- **Backend**: Rust (Tauri v2) handling all crypto operations
- **Crypto**: AES-256-GCM content encryption, EME (ECB-Mix-ECB) filename encryption, scrypt + HKDF key derivation
- **IPC**: Tauri `invoke()` — in-process, not over network sockets
- **Media**: Custom `vaultmedia://` protocol for streaming decrypted video/audio with HTTP Range support

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

## Tests

```bash
cd src-tauri
cargo test
```

232 tests covering:

| Module | Tests | What's covered |
|--------|------:|----------------|
| `crypto::content` | 27 | AES-256-GCM encrypt/decrypt roundtrips, block boundaries, wrong key rejection, corrupted data detection, nonce construction, size calculations |
| `crypto::filename` | 24 | EME filename encrypt/decrypt, PKCS#7 padding, base64 variants (raw64/padded), Unicode names, long name hashing |
| `crypto::eme` | 16 | ECB-Mix-ECB roundtrips, determinism, key/tweak sensitivity, GF(2^128) multiplication, avalanche effect |
| `crypto::streaming` | 14 | Streaming decryption, seek (start/end/current), cross-block reads, cache hits, invalid file handling |
| `crypto::kdf` | 12 | scrypt key derivation, HKDF sub-key derivation, wrong password detection, key independence |
| `crypto::config` | 11 | Config parsing, version/flag validation, error cases |
| `crypto::diriv` | 7 | Per-directory IV creation, reading, length validation |
| `vault::state` | 23 | Lock/unlock transitions, auto-lock timing, key access callbacks, media cache lifecycle, concurrent access, mlock integration, repeated lock/unlock resource leak testing |
| `vault::cache` | 18 | LRU eviction, size tracking, zeroize-on-drop, mlock/munlock of cache entries |
| `security::locked_key` | 8 | XOR masking, use_key/use_key_mut, mask re-randomization, memory scan resistance, clone isolation |
| `security::mlock` | 4 | mlock/munlock lifecycle, heap buffers, zero-length edge case |
| `security::coredump` | 2 | Core dump disabling, idempotency |
| `lib` (MIME, Range) | 22 | MIME type detection, HTTP Range header parsing, edge cases |
| Integration: `vault_ops` | 25 | Full vault CRUD on real encrypted structures: create/read/write/rename/delete/copy/search, nested directories, Unicode names, plaintext-not-on-disk verification |
| Integration: `memory_security` | 19 | Zeroize verification via raw pointers, mlock lifecycle, key isolation, no plaintext leakage in ciphertext/filenames, scrypt brute-force resistance, XOR-masked key roundtrips |

## License

MIT
