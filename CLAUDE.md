# CLAUDE.md — KoboldCpp Launcher

A modern launcher for KoboldCpp / Esobold, written in Rust with Tauri 2, replacing the
Tkinter starter in `koboldcpp.py`. It writes `.kcpps` configs and starts the existing `koboldcpp`
binary; it does **not** reimplement the server. Lite stays unchanged.

## Read first
1. [docs/DESIGN-NOTES.md](docs/DESIGN-NOTES.md) — user stories, decisions, Hugging Face API details, open questions.
2. [mockups/launcher/kobold-launcher.html](mockups/launcher/kobold-launcher.html) — clickable mockup
   (published privately at https://claude.ai/artifact/WGDP3KyHvzfKik8vvoe6MP; to update that page from
   a new session, read it by URL first, then publish with that `url`).

## Related code
- Esobold clone: `/Users/peter/Developer/KoboldAI Lite and RPMod and BlackLite/esobold`
  (`koboldcpp.py`: launch options via `add_argument`, Tk GUI in `show_gui`, HF search in `display_hf`).
- KoboldCpp templates: `hf.co/koboldcpp/newbie-templates`, `hf.co/koboldcpp/popular-templates`.

## Code map
- `src-tauri/src/`: `config/` (option table `args.json` from upstream, `.kcpps` model, CLI view),
  `hardware/` (GPU tools; pure parsers in `parse.rs`), `gguf.rs`, `fit.rs` (KoboldCpp's layer
  estimate, quant pick), `hf/` (sources, ranking, repo files, templates), `download.rs`, `binary.rs`
  (GitHub releases), `server.rs` (process), `library.rs`, `store.rs`, `commands.rs` (all Tauri commands).
- `src/`: `api.ts` (typed commands; browser fallback via `mock.ts`), `screens/`, `dialogs/`,
  `server.ts`, `downloads.ts`, `state.ts`. Rust does the work; TS renders.
- Upstream reference for options and formats is **LostRuins' `koboldcpp.py`** (the binary we run), not Esobold.

## Working notes
- In the Claude sandbox: run cargo with `CARGO_HOME=$TMPDIR/cargo-home`; network tests need
  `KL_CA_BUNDLE=/etc/ssl/cert.pem`; the real KoboldCpp binary can't start inside the sandbox
  (PyInstaller semaphores), so the e2e test must run in a normal terminal.
- UI check without Tauri: `.claude/launch.json` (Vite on 1420) + browser pane.

## Rules
- License: AGPL-3.0 (`LICENSE`), same as KoboldCpp. Reimplement ideas, don't copy proprietary code.
- No curated model list of our own; recommendations come from KoboldCpp's templates and the
  user's sources (see design notes).
- No lab or vendor logos for model families (letter badges); the GitHub mark only as a link to GitHub.
- Never commit secrets (`.env`, keys, tokens); check `git diff --cached` before every commit.
