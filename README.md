# VaultBox

A desktop application for browsing gocryptfs-encrypted vaults without mounting them. All decryption happens in-process memory — no FUSE mount, no temp files on disk, no plaintext exposed to other apps.

Cryptographic compatibility with gocryptfs v2 is verified by cross-validation tests: files encrypted by [gocryptfs](https://github.com/rfjakob/gocryptfs) are decrypted by VaultBox and vice versa.

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

| | gocryptfs FUSE mount | VaultBox |
|---|---|---|
| Plaintext access | Any process can `open("/mnt/vault/file")` | Only VaultBox process |
| Malware with user privileges | Reads files like normal files | macOS/Linux: blocked by OS |
| Kernel page cache | Decrypted pages cached by kernel | No kernel cache |
| Visibility | Mount point visible via `mount`, `df` | No mount point |

### Memory protection

| Protection | How | Platform |
|------------|-----|----------|
| XOR-masked keys | Keys stored as `masked = key ⊕ random_mask`; unmasked only during crypto operations (microseconds), then re-masked with fresh random pad | All |
| mlock | All key material and plaintext cache entries pinned in RAM via `mlock()` / `VirtualLock()` — never swapped to disk | All |
| Zeroize on drop | Keys, passwords, decrypted buffers zeroed via volatile writes (`zeroize` crate) — compiler cannot optimize away | All |
| Core dumps disabled | `setrlimit(RLIMIT_CORE, 0)` at startup | Unix |
| Anti-ptrace | `prctl(PR_SET_DUMPABLE, 0)` — blocks `/proc/pid/mem` reads and `ptrace` attach from same-user processes | Linux |

### Password handling

| Layer | Type | Zeroed after use? |
|-------|------|-------------------|
| `useSecurePassword` hook | `number[]` (mutable ref, not a JS string) | Yes — `fill(0)` on consume |
| IPC transport | `Uint8Array` → `number[]` | Yes — both zeroed in `finally` |
| Rust command handler | `Vec<u8>` → `String` for scrypt | Yes — both `.zeroize()` after KDF |

### Network isolation

| Layer | Mechanism |
|-------|-----------|
| CSP `connect-src` | `'self' ipc: http://ipc.localhost vaultmedia:` — blocks all external HTTP/WebSocket |
| No plugins with network access | `opener` plugin removed; only `dialog` plugin remains |
| Rust code | No `reqwest`, `hyper`, or any HTTP client in dependencies |

### Filesystem restrictions

| Mechanism | What it does |
|-----------|-------------|
| Tauri capabilities | Minimal: `core:default` + `dialog:allow-open` + `dialog:allow-save` only |
| `validate_external_path()` | Import/export blocked for `/etc`, `/System`, `/usr`, `/bin`, `/proc`, `/sys`, all hidden directories (`.ssh`, `.gnupg`, `.aws`, `.config`, etc.) |
| Vault operations | Path resolution constrained to `vault_path` — no traversal outside vault directory |

### Known limitations

- WebView holds internal copies of DOM input values and rendered content — not controllable from application code
- JavaScript strings are immutable and GC'd, not zeroized — decrypted text in open editor tabs persists in JS heap until GC collects
- On Windows, `ReadProcessMemory` from a same-user process can read VaultBox memory; XOR masking makes this harder but not impossible for a targeted attacker
- CodeMirror editor maintains internal copies of document content in JS heap

## Architecture

- **Frontend**: React 19 + TypeScript + Tailwind CSS 4 + CodeMirror 6
- **Backend**: Rust (Tauri v2) handling all crypto operations
- **Crypto**: AES-256-GCM (128-bit nonces) content encryption, EME (ECB-Mix-ECB) filename encryption, scrypt + HKDF-SHA256 key derivation
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

334 tests across 7 test suites:

| Suite | Tests | What's covered |
|-------|------:|----------------|
| Unit tests (`--lib`) | 203 | AES-256-GCM content encryption (16-byte nonces, 24-byte AAD), EME filename encryption, HKDF key derivation with known vectors, scrypt KDF, config parsing, streaming decryption, vault state management, LRU cache with mlock/zeroize, XOR-masked key storage, core dump prevention |
| `crypto_pure` | 78 | HKDF known vectors from gocryptfs, real vault KDF validation, block size/offset monotonicity, range splitting, content roundtrips (0B–10MB), filename roundtrips (dot names, Unicode, 300-char), AAD format verification, corruption detection, concurrent encrypt/decrypt, performance benchmarks, security edge cases |
| `vault_ops` | 25 | Full vault CRUD on real encrypted structures: create/read/write/rename/delete/copy/search, nested directories, Unicode names, plaintext-not-on-disk verification |
| `memory_security` | 19 | Zeroize verification via raw pointers, mlock lifecycle, key isolation, no plaintext leakage in ciphertext/filenames, scrypt brute-force resistance, XOR-masked key roundtrips |
| `gocryptfs_compat` | 6 | Cross-validation with real gocryptfs binary: VaultBox decrypts gocryptfs-created files, gocryptfs decrypts VaultBox-created files, config/key derivation compatibility |
| `debug_kdf` | 3 | Step-by-step KDF comparison with Go output (scrypt → HKDF → GCM → master key), pure HKDF vector validation |

### Cross-validation (requires FUSE)

Three tests require macFUSE/FUSE to mount a gocryptfs filesystem:

```bash
# Set gocryptfs binary path (or add to PATH):
export GOCRYPTFS_BIN=/path/to/gocryptfs

cargo test --test gocryptfs_compat -- --ignored --nocapture
```

These tests verify bidirectional compatibility:
- **gocryptfs → VaultBox**: Create files with gocryptfs mount, decrypt filenames and content with Rust
- **VaultBox → gocryptfs**: Create files with VaultBox Rust code, mount with gocryptfs and read back

## License

MIT
