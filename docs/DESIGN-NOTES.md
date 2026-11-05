# KoboldCpp Launcher: design notes

Brainstorm and decisions from the first planning chat (2026-11-04), moved here from the
KLITE-RPmod project. See "Implementation" below for what has been built since.

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

## Implementation (2026-11-05)

- **Uncensored label:** the repo ID contains uncensored, abliterated, unfiltered, decensored,
  orthogonalized, heretic or thedrummer (TheDrummer's models and others' quants of them), or has
  the word nsfw / erp on its own (so "enterprise" doesn't count), or the tag uncensored,
  not-for-all-audiences or nsfw.
- **Default sources, version 2:** Gryphe, PocketDoc, Sao10K, zerofata, BeaverAI, LatitudeGames,
  anthracite-org, MarinaraSpaghetti, NeverSleep, ToastyPigeon (on, community finetunes).
  Existing settings get them once (`sourcesVersion`); a removed one stays removed. Sources match
  by uploader, so accounts with few GGUF uploads (PocketDoc 0, Gryphe 2, Sao10K 2 in Nov 2026)
  add little: their models arrive as quants by others.
- **Default sources, version 3:** mradermacher (quants of nearly everything, static and imatrix
  "i1" repos) and DavidAU (own finetunes and merges), both off by default.
- **Discover search follows the sources:** only uploaders whose source is on (100 matches are
  fetched, most downloaded first); matches from sources that are off are counted per uploader
  and named in a note with a link to Model Sources. Changing the sources searches again.
- **Default sources, version 4:** lmstudio-community and ggml-org (official quants of well-known
  models; ggml-org also ships MTP files), on.
- **KoboldCpp output on a pseudo-terminal (macOS, Linux):** its C code prints with C stdio,
  which holds output back in a pipe (and loses it when stopped): no prompt/generation lines,
  no "Detected MTP draft head". With a pseudo-terminal it writes line by line. Color codes are
  stripped. Windows still uses pipes.

First full version built in six phases (commits on `main`). Decisions made on the way:

- **Binary:** official KoboldCpp releases only (`LostRuins/koboldcpp`); Esobold may become a second
  channel later. Asset per platform: `koboldcpp-mac-arm64`; `koboldcpp.exe` with NVIDIA, else
  `koboldcpp-nocuda.exe`, `koboldcpp-oldpc.exe` without AVX2; Linux x64 the same pattern. Downloads
  are checked against the SHA-256 digest GitHub publishes per asset. No build exists for Intel Macs.
- **Option table** comes from upstream `koboldcpp.py` at the release tag (183 options in v1.124.2),
  not from Esobold; extracted with `ast`, never executed.
- **Sparse `.kcpps`:** KoboldCpp fills missing keys with defaults and migrates old ones on load, so
  the launcher keeps every key it reads (also unknown ones) and only adds what the user changes.
  Launch is always `koboldcpp --config <file> --skiplauncher`; a config without any model is refused
  (KoboldCpp would open its own Tk window).
- **Downloads by the launcher, not by KoboldCpp:** template URLs are downloaded first (progress,
  resume, cancel), then replaced by local paths. `downloaddir` is set to the models folder as a
  fallback.
- **Fit:** local files use KoboldCpp's own layer estimate (`autoset_gpu_layers`); Hugging Face lists
  use sizes only (weights + context overhead). Apple Silicon: Metal budget ≈ ⅔ of RAM (¾ above
  36 GB), all or nothing. At launch `--gpulayers -1` (autofit) unless the user sets a number.
- **Quant pick:** Q6_K → Q5_K_M → Q4_K_M (Q8_0 for models under 3 GB); then Q4_K_M partly on CPU;
  then a smaller quant marked "Smaller quant". F16/BF16/Q5+ never count as fallback.
- **Popular list** shows text and vision-chat models only (image/voice come from templates).
  Uploader subfolders and files that are helpers (`MTP/`, `dspark`, drafts, imatrix) are skipped;
  quant names are found anywhere in a file name (Google: `gemma-4-E4B_q4_0-it.gguf`).
- **Official** = uploaded by the model maker's own account (known maker list); everything else is
  labeled "Quant by <uploader>" (no separate finetune label in the UI).
- **Sources:** each maker is its own built-in source, on by default (google, Qwen, zai-org,
  deepseek-ai, mistralai, meta-llama, openai, nvidia), plus unsloth; bartowski and TheDrummer
  optional. Old settings are migrated (the former combined "labs" entry is replaced).
- **Families and Top:** the family row maps each family to its maker. "Top" shows, per maker whose
  source is on, its best model that fits, in the chosen order (most downloaded by default;
  trending, most liked, newest). The maker's own upload wins; quants of its models are used only
  when the maker has no fitting GGUF of its own (DeepSeek, Meta and OpenAI publish none).
- **Quant pick:** other 4-bit quants (e.g. Google's QAT Q4_0) are a normal choice; "Smaller quant"
  only for 3-bit and below.
- **Library folders** are added with a choice "Add recursively from the folder" (default on: all
  subfolders, links followed, loops guarded); off = only files directly in the folder. Our own
  downloads folder is always recursive (`<uploader>/<repo>/`). LM Studio keeps models in
  `~/.lmstudio/models/<uploader>/<repo>/` (its `hub/models` folder holds only catalog files).
- **Runs here?** Each library card says whether the model runs on this computer, checked in order:
  readable GGUF header, all split parts present, architecture known to the installed KoboldCpp
  (list from `src/llama-arch.cpp` at the release tag, `tools/extract_archs.py`; only for text,
  vision and embedding models), memory (KoboldCpp's layer estimate with the GGUF layout at 16K
  context). Green Start / "Try anyway" (too big, architecture unknown to a newer build) / disabled.
  A filter shows only models that run here.
- **MLX and safetensors** model folders (config.json + *.safetensors, e.g. LM Studio's MLX
  downloads) are listed in the library but marked as not runnable: KoboldCpp is llama.cpp-based
  and loads GGUF only.
- **Vision / MTP checkboxes:** only where available, both on by default; a choice the user makes
  is kept per model (localStorage, only explicit choices stored). Vision off = the mmproj is
  neither downloaded nor loaded. If a needed file is missing (e.g. Vision switched on), the green
  Start turns back into "Get and start", which downloads only what is missing.
- **MTP comes in two forms** (checked in KoboldCpp v1.124.2 `kcpp_src/gpttype_adapter.cpp`):
  - *Built in* (Qwen 3.5+, `qwen35`): the model file has `<arch>.nextn_predict_layers` > 0 and
    `nextn` tensors; `--usemtp` builds MTP from them, nothing to download. The repo's 1.3 GB
    `MTP/mtp-…` file is only a copy of those layers for llama.cpp and is not used.
  - *Separate head* (Gemma 4, arch `gemma4-assistant`, `nextn_predict_layers` = 4): the model
    file has none, so `--usemtp` alone does nothing. The head (~93 MB Q8_0) goes in as
    `--draftmodel`; KoboldCpp detects the MTP head and switches to MTP decoding.
  - Only repos that ship `MTP/` or `mtp-` files are checked: the first megabyte of the model file
    and of the head is read with an HTTP range request (`gguf::read_header_prefix`, stops at the
    tokenizer keys; cached per URL). Head choice: Q8_0 (unsloth's recommendation), else the
    smallest. Rows show "MTP: built in" or "MTP: 93 MB". A repo without MTP files whose model has
    built-in layers (e.g. other uploaders' Qwen 3.5+ quants) gets no MTP option (not checked).
  - Library: `mtp-*.gguf` / `*-assistant` files are add-ons (no card of their own), paired with a
    model in the same folder or its `MTP/` subfolder that has no MTP layers itself.
  - KoboldCpp warns that MTP may not work well with a vision projector; rows with both ticked say so.
- **Appearance** (Advanced setup): "Light" or "KoboldAI Lite". The Lite scheme uses Lite's default
  color scheme "Modern" (`"Default (Cedo)"` in Lite's `themes.js`; the "Aqua" values in Lite's CSS
  are only its fallback). Stored in localStorage, applied before the first render.
- **Speed (tokens/s):** estimated from memory bandwidth, since each token reads the (active)
  weights once: time = GPU share ÷ (GPU GB/s × efficiency) + CPU share ÷ (RAM GB/s × 0.5) + a
  small fixed cost; efficiency 0.7 (discrete GPU) / 0.6 (Apple). Bandwidth per GPU from a table
  of known models (`hardware::bandwidth_of`); MoE uses the active share from names like "A3B".
  RAM bandwidth: Apple Silicon from the chip table; otherwise from the reported memory type,
  rate and module count (Windows `Win32_PhysicalMemory`, Linux `dmidecode -t 17` (root only),
  Intel Macs `system_profiler SPMemoryDataType`): MT/s × 8 bytes × channels (desktop max 2,
  Threadripper/Xeon/EPYC max 8; soldered LPDDR = 128-bit bus). The CPU chip shows the type,
  e.g. "32 GB DDR4-3200 RAM". No guessed values: an unknown RAM type shows "RAM (type N/A)", and
  when a bandwidth the estimate needs is unknown (RAM for CPU layers, an unlisted discrete GPU)
  the speed shows "N/A" until a measured speed exists. Integrated GPUs use the RAM bandwidth.
  Calibrated on an M2 (4B Q4 ≈ 15–20 t/s; measured 13.3) and RTX 4090 (8B Q4 ≈ 110 t/s).
  While a model runs, `/api/extra/perf` `last_eval_speed` is recorded per model file (answers of
  16+ tokens) and shown as "measured" instead of the estimate. Library sorts by speed (measured
  first), last used, name or size.
- **Discover on Hugging Face** answers like the popular list (same rows, quant pick, options,
  Start / Get and start); results that don't fit are hidden and counted.
- **Downloaded:** models whose files are already in the models folder get a green "Start".
- **Remove** moves files to the system Trash, never a hard delete; move and remove only work inside
  library folders.
- **LAN access:** the advanced page writes `host 127.0.0.1` when "Allow other devices" is off
  (KoboldCpp's own default listens on all interfaces).

Not done yet: measured speed per model ("Speed here" is an estimate), live dashboard, tray mode,
model swap without restart, Esobold channel, Windows/Linux checks on real hardware (detection is
tested on recorded tool output only), app icon.

## About the mockup

`mockups/launcher/kobold-launcher.html` is a page fragment (no `<html>`/`<head>`), written for the
artifact viewer; it also opens directly in a browser. All model data, sizes and speeds are examples,
except the template names and the MidSpec-Chatbot contents. The command line shown is close to
KoboldCpp's real syntax but must be generated from the same code that writes `.kcpps`.
