// Per-model choices the user can switch on a card: load the vision file, use MTP layers.
// Kept in localStorage (a view preference); the launch config is built from them at start.

export interface ModelOpts {
  /** Load the vision file (mmproj) so the model can see images. Costs memory. */
  vision: boolean;
  /** Use the model's MTP layers for faster answers (KoboldCpp --usemtp). Costs memory. */
  mtp: boolean;
}

/** Both on unless the user switches them off for a model (that choice is kept). */
const DEFAULTS: ModelOpts = { vision: true, mtp: true };
// Only choices the user made are stored, so a default stays a default.
const key = (id: string) => `kl-opt2:${id}`;
/** Earlier versions stored both values on any change; only their Vision choice was a real one. */
const oldKey = (id: string) => `kl-opts:${id}`;

function stored(id: string): Partial<ModelOpts> {
  try {
    const raw = localStorage.getItem(key(id));
    if (raw) return JSON.parse(raw);
    const old = localStorage.getItem(oldKey(id));
    if (old) {
      const v = JSON.parse(old).vision;
      return typeof v === "boolean" ? { vision: v } : {};
    }
  } catch {
    /* storage unavailable or broken entry */
  }
  return {};
}

export function getOpts(id: string): ModelOpts {
  return { ...DEFAULTS, ...stored(id) };
}

export function setOpts(id: string, patch: Partial<ModelOpts>) {
  try {
    localStorage.setItem(key(id), JSON.stringify({ ...stored(id), ...patch }));
  } catch {
    /* storage unavailable: the choice lasts until the app restarts */
  }
}
