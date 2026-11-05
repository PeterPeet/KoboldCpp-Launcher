// Per-model choices the user can switch on a card: load the vision file, use MTP layers.
// Kept in localStorage (a view preference); the launch config is built from them at start.

export interface ModelOpts {
  /** Load the vision file (mmproj) so the model can see images. Costs memory. */
  vision: boolean;
  /** Use the model's MTP layers for faster answers (KoboldCpp --usemtp). Costs memory. */
  mtp: boolean;
}

const DEFAULTS: ModelOpts = { vision: true, mtp: false };
const key = (id: string) => `kl-opts:${id}`;

export function getOpts(id: string): ModelOpts {
  try {
    const raw = localStorage.getItem(key(id));
    return raw ? { ...DEFAULTS, ...JSON.parse(raw) } : { ...DEFAULTS };
  } catch {
    return { ...DEFAULTS };
  }
}

export function setOpts(id: string, patch: Partial<ModelOpts>) {
  try {
    localStorage.setItem(key(id), JSON.stringify({ ...getOpts(id), ...patch }));
  } catch {
    /* storage unavailable: choice lasts until the next render only */
  }
}
