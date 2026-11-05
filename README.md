# KoboldCpp Launcher

A modern desktop launcher for [KoboldCpp](https://github.com/LostRuins/koboldcpp), written in Rust
with Tauri. It replaces the Tkinter starter: it finds a model that fits your PC, writes a `.kcpps`
settings file and starts the official KoboldCpp program. KoboldCpp itself and its web UI (Lite) are
unchanged.

## What it does

- **Start screen:** detects your GPUs, CPU and memory, shows KoboldCpp's own ready-made templates for
  your PC, and a ranked list of popular models from your Hugging Face sources. One click downloads
  what is needed and starts KoboldCpp. The best quantization that fits is picked for you.
- **Model library:** every `.gguf` in your folders as a card (type, size, context, layers, drive,
  last used), with filters, move to another folder or drive, and remove to the Trash. A second tab
  searches all GGUF models on Hugging Face.
- **Advanced setup:** backend and GPUs, layers, split across several GPUs with memory bars, context,
  cache, image and voice models, network, presets. The matching command line is shown and can be
  copied.
- **Sources:** which Hugging Face uploaders to use (KoboldCpp templates, the labs' own uploads,
  unsloth, bartowski, TheDrummer, anyone), in priority order, with official, finetune and
  uncensored switches. Export and import as a file.
- **KoboldCpp updates:** installs the right official release for your PC, checks its SHA-256, and
  updates in place (a running model is restarted).
- **Clean shutdown:** closing the launcher stops the KoboldCpp it started; a KoboldCpp left over from
  a launcher that was killed is ended at the next start. Both logs are under Advanced setup › Logs.

## Where files live

Everything goes into the app data folder (on macOS
`~/Library/Application Support/io.github.peterpeet.koboldlauncher/`):

| Path | Content |
|---|---|
| `settings.json` | launcher settings and sources |
| `bin/<version>/` | the KoboldCpp program |
| `models/` | downloaded models (default; more folders can be added in the library) |
| `presets/*.kcpps` | saved presets |
| `runs/last-run.kcpps` | settings of the last start |
| `logs/koboldcpp.log` | everything KoboldCpp printed during the last start |
| `logs/launcher.log` | what the launcher did: starts, stops, downloads, updates, errors |
| `cache/` | Hugging Face answers (24 h) |

## Development

Tauri 2 app: Rust in `src-tauri/`, TypeScript + Vite in `src/`.
Design notes: [docs/DESIGN-NOTES.md](docs/DESIGN-NOTES.md).

Prerequisites: Rust via [rustup](https://rustup.rs), Node.js 20+, Xcode (macOS).

```bash
npm install
npm run tauri dev     # run the app with hot reload
npm run tauri build   # build a release bundle
```

Opened in a normal browser (`npm run dev`), the UI runs on example data and shows a
"Browser preview" badge.

Tests:

```bash
cd src-tauri
cargo test                                    # unit and integration tests, offline
cargo test --test hf_live -- --ignored        # live Hugging Face checks
KL_E2E_DIR=/tmp/kl-e2e cargo test --test e2e -- --ignored --nocapture
```

The last one downloads the real KoboldCpp release and a 0.4 GB model, starts it, checks its API and
stops it. `KL_CA_BUNDLE=/path/to/bundle.pem` makes all downloads trust only that certificate bundle
(for corporate proxies or sandboxes without keychain access).

When KoboldCpp adds launch options, refresh the option table from its source:

```bash
python3 -I tools/extract_args.py path/to/koboldcpp.py v1.xxx > src-tauri/src/config/args.json
```

License: AGPL-3.0, same as KoboldCpp.
