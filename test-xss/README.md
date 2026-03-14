# XSS Test Files

Import these into a vault and verify nothing executes.

## What to check

| File | Open in | Expected |
|------|---------|----------|
| `evil.svg` | Image viewer | Red square, NO alert, title stays unchanged |
| `evil.md` | Text editor + Preview | `javascript:` links stripped, `<script>` shows as text |
| `evil.html` | Text editor (CodeMirror) | Shows as source code, NOT rendered as HTML |
| `evil.txt` | Text editor | Shows as plain text |
| `evil.json` | Text editor | Shows as JSON source |

## Red flags (if any of these happen, there's a vulnerability)

- Browser alert dialog appears
- Page title changes to anything with "XSS"
- Network request to evil.com (check Network tab)
- Tauri API access from file content
- File system access (reading /etc/passwd etc)
