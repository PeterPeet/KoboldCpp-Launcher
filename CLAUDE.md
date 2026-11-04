# CLAUDE.md — KoboldCpp Launcher

A modern launcher for KoboldCpp / Esobold, written in Rust (Tauri is the current idea), replacing the
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

## Rules
- License: AGPL-3.0 (`LICENSE`), same as KoboldCpp. Reimplement ideas, don't copy proprietary code.
- No curated model list of our own; recommendations come from KoboldCpp's templates and the
  user's sources (see design notes).
- No lab or vendor logos for model families (letter badges); the GitHub mark only as a link to GitHub.
- Never commit secrets (`.env`, keys, tokens); check `git diff --cached` before every commit.
