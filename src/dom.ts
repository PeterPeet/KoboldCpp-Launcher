// Small DOM helpers shared by all screens.

export type Kid = Node | string | number | null | undefined | false | Kid[];
export type Attrs = Record<string, unknown> | null;

export const $ = <T extends Element = HTMLElement>(sel: string, root: ParentNode = document) =>
  root.querySelector<T>(sel);

/** Create an element. `on*` attributes become event listeners, `html` sets innerHTML. */
export function h(tag: string, attrs: Attrs = {}, ...kids: Kid[]): HTMLElement {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs ?? {})) {
    if (v == null || v === false) continue;
    if (k === "class") el.className = String(v);
    else if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2), v as EventListener);
    else if (k === "html") el.innerHTML = String(v);
    else el.setAttribute(k, v === true ? "" : String(v));
  }
  append(el, kids);
  return el;
}

function append(el: HTMLElement, kids: Kid[]) {
  for (const k of kids) {
    if (k == null || k === false) continue;
    if (Array.isArray(k)) append(el, k);
    else el.append(typeof k === "object" ? k : String(k));
  }
}

/** Checkbox input with a change handler. */
export function checkbox(id: string, checked: boolean, onChange: (v: boolean) => void, extra: Attrs = {}) {
  const i = h("input", { type: "checkbox", id, checked, ...extra }) as HTMLInputElement;
  i.addEventListener("change", () => onChange(i.checked));
  return i;
}

/** Toggle switch (styled checkbox), optionally with a text label. */
export function toggle(id: string, checked: boolean, onChange: (v: boolean) => void, label?: string) {
  return h("label", { class: "switch", for: id }, checkbox(id, checked, onChange, label ? {} : { "aria-label": id }), label);
}

/** Select with [value, label] options. */
export function select(id: string, value: string | number, opts: [string | number, string][], onChange: (v: string) => void) {
  const s = h("select", { id }, ...opts.map(([v, l]) => h("option", { value: v, selected: String(v) === String(value) }, l))) as HTMLSelectElement;
  s.addEventListener("change", () => onChange(s.value));
  return s;
}

let toastTimer: number | undefined;
export function toast(msg: string) {
  const r = $("#toastRoot")!;
  r.replaceChildren(h("div", { class: "toast", role: "status" }, msg));
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => r.replaceChildren(), 2600);
}

export const gb = (n: number) => (n < 10 ? n.toFixed(1) + " GB" : Math.round(n) + " GB");
export const fmt = (n: number) => (n >= 1e6 ? (n / 1e6).toFixed(1) + "M" : n >= 1e3 ? Math.round(n / 1e3) + "k" : String(n));
