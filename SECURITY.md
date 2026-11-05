# Security

## Reporting a problem

Please report security problems privately through GitHub: **Security › Report a vulnerability**
on this repository, not as a public issue.

## What the launcher does to stay safe

- **Downloads** are `https://` only. KoboldCpp itself is checked against the SHA-256 that GitHub
  publishes for the release. File and folder names from URLs are reduced to safe characters, so
  a download can't land outside the models folder.
- **Settings from outside** (KoboldCpp templates from Hugging Face, imported `.kcpps`/`.kcppt`
  files) can't run programs, read or write other files, join other networks or open the server
  to other computers. Those options are removed (`src-tauri/src/config/safety.rs`), and an import
  says what it left out. A few of them (LAN access, remote tunnel, password) stay available
  in Advanced setup when you set them yourself.
- **Every start** removes options the launcher never offers (shell commands on start, MCP
  tool servers, agents, admin and router mode, Horde, RPC, file export and preload, disabled
  certificate checks), whatever the config came from.
- **Files on disk:** the launcher moves or deletes only `.gguf` models inside your library
  folders (deleting means the Trash), loads presets only from its presets folder, and runs only a
  program whose name says KoboldCpp.
- **Processes:** it stops only KoboldCpp programs started from its own folder.
- **The window** loads only the app's own files (Content Security Policy). Names and notes from
  Hugging Face and GitHub are shown as text, never as HTML.
- **No secrets** are stored. The launcher has no accounts, and it sends nothing except requests
  to Hugging Face and GitHub.

## Known limits

- The installers are not code-signed yet (macOS Gatekeeper and Windows SmartScreen warn on
  first start).
- KoboldCpp's own web server (KoboldAI Lite and its API) is KoboldCpp's responsibility; by default
  it listens on this computer only.
