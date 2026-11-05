# KoboldCpp-Launcher
An improved KoboldCpp Launcher written in Rust

## Development

Tauri 2 app: Rust backend in `src-tauri/`, TypeScript + Vite frontend in `src/`.
Design notes: [docs/DESIGN-NOTES.md](docs/DESIGN-NOTES.md).

Prerequisites: Rust via [rustup](https://rustup.rs), Node.js 20+, Xcode (macOS).

```bash
npm install
npm run tauri dev     # run the app with hot reload
npm run tauri build   # build a release bundle
```

License: AGPL-3.0, same as KoboldCpp.
