# KoboldCpp Launcher: design notes

Brainstorm and decisions from the first planning chat (2026-11-04), moved here from the
KLITE-RPmod project. Nothing is built yet; this is the starting point.

- **Clickable mockup:** [mockups/launcher/kobold-launcher.html](../mockups/launcher/kobold-launcher.html)
  (published privately at https://claude.ai/artifact/WGDP3KyHvzfKik8vvoe6MP)
- **Chat transcript:** exported as a zip to `~/Downloads` (see the end of this file)

## Goal

A modern launcher for KoboldCpp / Esobold, written in Rust, replacing the Tkinter starter in
`koboldcpp.py`. Lite (the web UI) stays as it is; the server keeps serving it unchanged.

## Scope: launcher first, not a server rewrite

What the code base looks like (measured in the Esobold clone, `esolithe/esobold`):

| Part | Size | Plan |
|---|---|---|
| `koboldcpp.py`: API server (~150 routes), model loading, downloads, Horde, tunnel, ~200 launch options | 18.3k lines | keep |
| Tkinter launcher inside `koboldcpp.py` (`show_gui`, lines ~12763–15187) | ~2.4k lines | **replace** |
| `kcpp_agent.py`, tool-call parser | 5.5k lines | keep |
| C++ glue (`kcpp_src/`, `otherarch/*_adapter.cpp`) with a C interface (`expose.h`) | 13.6k lines | keep |
| llama.cpp, ggml, shaders | huge | keep |

GitHub counts the repo as 91% C++. Server and Lite alone are 59% JavaScript, 15% CSS, 14% Python, 6% C++.

**Estimates**
- Full Rust rewrite of the server: 10–16 developer-months (€120k–250k hired). With Claude: 3–6 months,
  roughly $1,500–5,000 in tokens. Plus a permanent cost of porting every upstream release.
  The rewrite would stay AGPL (derived work).
- **Launcher only (chosen):** a Rust/Tauri app that writes `.kcpps` configs and starts the existing
  `koboldcpp` binary. 2–4 weeks with Claude, roughly $150–500 in tokens. Stays compatible with upstream.

## Three user stories (separate screens)

1. **Beginner, one click.** Detect hardware, show what this PC can run, click → download → start.
2. **Model library.** Local models as a card grid (inspired by the Novita model library): family badge,
   size, context, measured speed here, fit on this GPU, last used, drive. Filters for type, family,
   drive (with fill bars), features. Second tab: discover on Hugging Face (the Tk launcher already has
   an HF search: `hfsearch_get_help`, `display_hf` in `koboldcpp.py`).
3. **Advanced setup, single and multi-GPU.** Full system-settings style (Gnome / macOS). Hardware page
   with backend, GPU switches, draggable layer split, memory per GPU, split mode, main GPU, which GPU
   runs the image model, and the matching command line (copyable).

## Decisions

**Where recommendations come from**
- No curated list of our own.
- **KoboldCpp's own templates first:** `hf.co/koboldcpp/newbie-templates` (LowSpec / MidSpec /
  HighSpec × Chatbot, CodingAgent, ImageGen, MusicGen, VideoGen) and `hf.co/koboldcpp/popular-templates`.
  A `.kcppt` file is a complete launch config with model URLs. Example `MidSpec-Chatbot.kcppt`:
  Qwen3-VL-8B Q4_K_M + mmproj, Whisper, Kokoro TTS, bge-m3 embeddings, 24K context, `gpulayers -1`,
  web search on. Map detected hardware to a tier, but still run the memory calculation on the
  template's files (show "partial" instead of failing).
- **Then "popular" models** from Hugging Face signals, ranked across the user's sources:
  `score = (log10(downloads_30d) + 0.6·log10(likes)) × max(0.35, 0.5^(age_days/180))`.
  Filter by fit first, merge duplicates by `base_model`, cache the list for 24 h.
- `hf.co/KoboldAI` (old finetunes) and the 50 links in `colab.ipynb` are outdated, not used.

**Quantization:** beginners don't choose; the app picks the largest of Q6_K / Q5_K_M / Q4_K_M that
fits (smaller only as a marked fallback). Library and advanced setup show the alternatives.

**Hugging Face API** (checked live, no account needed)
- `GET /api/models?filter=gguf&pipeline_tag=text-generation&sort=trendingScore&expand[]=likes&expand[]=downloads&expand[]=createdAt&expand[]=gguf`
  → likes, 30-day downloads, release date, GGUF params / architecture / context length.
- `GET /api/models/{repo}/tree/main` → exact file size per quantization.
- `expand[]=baseModels` / `cardData.base_model` → base model and relation (`quantized`, `finetune`, …).

**Sources (APT-style)**
- The user manages a list of Hugging Face uploaders. Built-in: "KoboldCpp templates" and
  "Labs' own uploads" (repo author = base model author, e.g. `Qwen/…-GGUF`, `LiquidAI/…-GGUF`).
  Default: templates + labs + **unsloth**. Optional: bartowski, TheDrummer, any name.
- Components per source like APT's main/contrib: *Official*, *Finetunes*, *Uncensored*.
- Priority order: when two sources offer the same model, the higher one wins.
- Import / export the list as a small file to share setups.
- Measured: unsloth 393 GGUF repos (official only), bartowski 1,000+ (official plus community
  finetunes, incl. 20 "uncensored" and 51 "abliterated"), TheDrummer 106 (own finetunes).
  lmstudio-community and unsloth have no uncensored repos.

**Switches**
- "Official releases / All models". HF's `relation: "quantized"` is self-declared and unreliable
  (an uncensored finetune claimed it), so *official* = relation `quantized` **and** repo name equals
  the base model name plus a GGUF suffix.
- "Show uncensored finetunes": name contains uncensored / abliterated / unfiltered / decensored /
  orthogonalized, or tags `uncensored`, `not-for-all-audiences`. Default off on the beginner screen.

**Hardware choice (from mockup review):** the start screen lists each detected GPU as a tick box plus
the CPU (always on). Unticking all GPUs gives a CPU-only run; fits and speeds recalculate.

**Updates (from mockup review):** the version next to a small GitHub icon links to the repo. On start
the app checks GitHub releases (`LostRuins/koboldcpp`, latest v1.124.2 on 2026-10-31; Esobold:
`esolithe/esobold`, RMv1.36.3), shows "Update to vX" in the top bar, and updates in place
(download, verify, restart the running model). Switches: check on start, include pre-releases.
Open question: follow KoboldCpp or Esobold releases.

**Branding:** model families as coloured letter badges (no lab logos, trademarks). App logo:
`koboldai-logo.png` from `PeterPeet/koboldai.com` (`docs/img/koboldai-logo.png`).

## Other ideas (not decided)

- Live dashboard after start: tokens/s, GPU memory, context use, connected clients, log drawer.
- Tray / menu-bar mode with Start, Stop, Open Lite.
- QR code for remote access from a phone.
- Switch models without restarting (KoboldCpp admin / model swap).
- One-click benchmark stored with each preset; "Speed here" shows estimate (grey) vs measured.
- "Copy as command" everywhere; import existing `.kcpps`.
- Optional neon "loadout" theme (models as RPG gear).

## About the mockup

`mockups/launcher/kobold-launcher.html` is a page fragment (no `<html>`/`<head>`), written for the
artifact viewer; it also opens directly in a browser. All model data, sizes and speeds are examples,
except the template names and the MidSpec-Chatbot contents. The command line shown is close to
KoboldCpp's real syntax but must be generated from the same code that writes `.kcpps`.
