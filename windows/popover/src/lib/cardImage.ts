/**
 * Copying a provider card as an image.
 *
 * The route (proven in both tray engines by the spike behind this feature,
 * .soho/w96/reports/build-9-20261026T140708.md): clone the card, inline the
 * computed styles so the clone is self-contained, embed it in an SVG
 * `foreignObject` (XHTML namespace on the root, the SVG namespace re-declared
 * on nested icons — the data-URL SVG is re-parsed as XML), load the SVG in an
 * `<img>`, draw it scaled onto a `<canvas>`, and `toDataURL`. Self-contained
 * content (system fonts, inline local SVGs) does not taint the canvas in either
 * engine; anything cross-origin would raise a SecurityError here, which the
 * caller reports as a failed copy without ever asking the host.
 */

/** Fixed export scale: a 320 px card becomes a 960 px PNG. */
export const CARD_IMAGE_SCALE = 3;
/** No host answer within this window reads as a failed copy. */
export const IMAGE_COPIED_TIMEOUT_MS = 5000;
export const PNG_DATA_URL_PREFIX = "data:image/png;base64,";
/**
 * What an exported card never shows: the header's action buttons (one of them is
 * the copy trigger itself) and the reorder grip. Kept in step with the
 * `[data-exporting]` rules in index.css, which state the same policy.
 */
export const EXPORT_HIDDEN_SELECTOR = ".header-action, .grip";

export interface CardSvgParts {
  /** The card's self-contained HTML (namespacedCardHtml of the clone). */
  html: string;
  height: number;
  /** The opaque background's corner radius: the card's own radius. */
  radius: number;
  /** The opaque background: the theme's surface color. */
  surface: string;
  width: number;
}

/**
 * The SVG document the card rasterizes in: an opaque rounded background (the
 * export's surface color, the card's radius) with the card's HTML inside a
 * foreignObject. The XHTML namespace is on the wrapper — without it the XML
 * re-parse drops the HTML semantics and the card renders blank.
 */
export function buildCardSvg(parts: CardSvgParts): string {
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${parts.width}" height="${parts.height}">` +
    `<rect x="0" y="0" width="${parts.width}" height="${parts.height}" rx="${parts.radius}" fill="${parts.surface}"/>` +
    `<foreignObject width="100%" height="100%">` +
    `<div xmlns="http://www.w3.org/1999/xhtml" style="width:100%;height:100%">` +
    parts.html +
    `</div></foreignObject></svg>`
  );
}

/**
 * Re-namespace a serialized subtree for the SVG data URL: every `<svg>` icon
 * needs the SVG namespace re-declared or it inherits the XHTML one and vanishes.
 * (The HTML root gets its namespace from buildCardSvg's wrapper.)
 */
export function namespacedCardHtml(html: string): string {
  return html.replace(/<svg\b(?![^>]*xmlns)/g, '<svg xmlns="http://www.w3.org/2000/svg"');
}

/** The base64 body of a `toDataURL("image/png")` result. */
export function stripPngDataUrlPrefix(dataUrl: string): string {
  if (!dataUrl.startsWith(PNG_DATA_URL_PREFIX)) {
    throw new Error(`not a PNG data URL: ${dataUrl.slice(0, 40)}`);
  }
  return dataUrl.slice(PNG_DATA_URL_PREFIX.length);
}

/**
 * The pending copy-image sends, in FIFO order.
 *
 * What this protects is the association between a send and its own ack: the host
 * answers through one boolean hook (`window.__AIUB_IMAGE_COPIED__(ok)`), so the hook
 * always settles the OLDEST unanswered send — and the correction depends on the host
 * handling the IPCs in order and answering in the same order. Each host's event loop
 * is single-threaded, and the copy-image arm answers synchronously, so an ack always
 * belongs to the oldest send the host actually received.
 *
 * A send that times out is settled with a failure but stays in the queue as a
 * tombstone until its late ack arrives (where it is discarded silently): without the
 * tombstone, the late ack would settle the copy that started after it with a result
 * that is not its own.
 */
export interface CopyQueueSend {
  /**
   * Settle the send before the IPC is dispatched (the pre-send failure path): the host
   * never received a command, so no ack will ever come and the entry must LEAVE the
   * queue — a tombstone would swallow the next send's ack.
   */
  cancel: (ok: boolean) => void;
  /** Fire the send's timeout (the queue's own timer calls it; idempotent). */
  expire: () => void;
}

export interface CopyQueue {
  /**
   * Start a send. `onResult(ok)` settles it exactly once: with its own ack (before the
   * timeout) or with the timeout's false.
   */
  send: (onResult: (ok: boolean) => void, timeoutMs?: number) => CopyQueueSend;
  /** Consume the host's ack for the head of the queue; a response on an empty queue is ignored. */
  ack: (ok: boolean) => void;
  /** A send is waiting and not expired: the double-click lock. */
  isPending: () => boolean;
}

interface CopyQueueTimers {
  clear: (id: unknown) => void;
  set: (task: () => void, ms: number) => unknown;
}

/**
 * Build the pending-copy queue. `globalThis` timers by default so the same logic runs
 * in the browser; tests inject fake timers to drive the expirations directly.
 */
export function createCopyQueue(
  timers: CopyQueueTimers = {
    clear: (id) => globalThis.clearTimeout(id as number),
    set: (task, ms) => globalThis.setTimeout(task, ms),
  },
): CopyQueue {
  interface Entry {
    expired: boolean;
    onResult: (ok: boolean) => void;
    settled: boolean;
    timer: unknown;
  }
  const queue: Entry[] = [];

  const send = (onResult: (ok: boolean) => void, timeoutMs: number = IMAGE_COPIED_TIMEOUT_MS): CopyQueueSend => {
    const entry: Entry = { expired: false, onResult, settled: false, timer: undefined };
    queue.push(entry);
    entry.timer = timers.set(() => expire(), timeoutMs);
    const expire = (): void => {
      if (entry.settled) return;
      entry.expired = true;
      entry.settled = true;
      timers.clear(entry.timer);
      onResult(false);
      // The entry stays: its late ack must still be consumed — and discarded — here.
    };
    return {
      cancel: (ok: boolean): void => {
        if (entry.settled) return;
        entry.settled = true;
        timers.clear(entry.timer);
        const i = queue.indexOf(entry);
        if (i !== -1) queue.splice(i, 1);
        onResult(ok);
      },
      expire,
    };
  };

  const ack = (ok: boolean): void => {
    const head = queue[0];
    if (!head) return; // a response with no pending send is ignored
    queue.shift();
    if (head.expired) return; // its send already reported the timeout's failure
    timers.clear(head.timer);
    head.settled = true;
    head.onResult(ok);
  };

  const isPending = (): boolean => queue.some((entry) => !entry.expired);

  return { ack, isPending, send };
}

/**
 * The section to export, WITHOUT interpolating its id into a selector: a macOS account
 * label may carry a double quote, and an id like `anthropic@work"demo` would make
 * `[data-card-id="..."]` an invalid selector that throws before the copy starts.
 * Compare the attribute's value directly instead.
 */
export function findCardElement(root: ParentNode | null, cardId: string): HTMLElement | null {
  if (!root) return null;
  for (const candidate of root.querySelectorAll<HTMLElement>("[data-card-id]")) {
    if (candidate.getAttribute("data-card-id") === cardId) return candidate;
  }
  return null;
}

/**
 * How the clone is serialized into the SVG.
 *
 * The clone must be serialized as XML, not with `outerHTML`: the HTML serializer writes
 * U+00A0 as `&nbsp;` (among other HTML-only entities), and `&nbsp;` does not exist in an
 * XML document — the SVG data URL then fails to load and the copy reports a failure for
 * a card whose only fault is a non-breaking space in its name. The platform's
 * XMLSerializer writes XML escaping (`&amp;`, `&lt;`) and numeric references or literal
 * characters for the rest. Injectable so the pipeline is exercisable where no
 * XMLSerializer exists (bare node).
 */
export interface CardImageOptions {
  serialize?: (clone: Node) => string;
}

const defaultSerialize = (clone: Node): string => new XMLSerializer().serializeToString(clone);

export function serializeCardHtml(clone: Node, options?: CardImageOptions): string {
  return (options?.serialize ?? defaultSerialize)(clone);
}

/**
 * Rasterize one provider card (the `data-card-id` section) to PNG and return the
 * base64 body, without the `data:image/png;base64,` prefix. Never mutates the
 * card on screen: the work happens on a detached clone marked `data-exporting`.
 */
export async function cardImagePng(element: HTMLElement, options?: CardImageOptions): Promise<string> {
  const bounds = element.getBoundingClientRect();
  const width = Math.ceil(bounds.width);
  const height = Math.ceil(bounds.height);
  if (width <= 0 || height <= 0) throw new Error("the card has no size to export");

  const clone = element.cloneNode(true) as HTMLElement;
  // Marks the clone for the [data-exporting] rules in index.css; the clone is
  // detached, so those decisions are also applied inline below.
  clone.setAttribute("data-exporting", "true");
  inlineComputedStyles(element, clone);
  stripExportChrome(element, clone);
  const svg = buildCardSvg({
    html: namespacedCardHtml(serializeCardHtml(clone, options)),
    height,
    radius: cssPixelLength("--card-radius"),
    surface: resolvedCssVar("--export-surface"),
    width,
  });
  const dataUrl = await rasterizePng(svg, width, height);
  return stripPngDataUrlPrefix(dataUrl);
}

/** The root element's value for a CSS custom property (theme-scoped by the .dark class). */
function resolvedCssVar(name: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  if (!value) throw new Error(`missing ${name} token`);
  return value;
}

/** A CSS custom property as a pixel length: the export geometry, in the card's own units. */
function cssPixelLength(name: string): number {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const match = /^(-?\d*\.?\d+)px$/.exec(value);
  if (!match) throw new Error(`unresolvable ${name}: ${value}`);
  return Number(match[1]);
}

/**
 * Make the clone self-contained: every element's computed style — resolved (colors
 * computed, custom properties already substituted, box-sizing border-box), inherited
 * values included — moves onto the clone inline. Paired by document order; cloneNode
 * keeps the same shape.
 *
 * Property by property on purpose: Chromium's computed-style `cssText` is empty and
 * WebKit's computed style exposes no enumerable property names, so the per-property
 * `length`/`item()` walk is the one path that works in both tray engines.
 */
function inlineComputedStyles(source: HTMLElement, clone: HTMLElement) {
  const originals = [source, ...source.querySelectorAll("*")];
  const copies = [clone, ...clone.querySelectorAll("*")];
  for (let i = 0; i < originals.length && i < copies.length; i++) {
    const copy = copies[i] as HTMLElement;
    const computed = getComputedStyle(originals[i]);
    for (let property = 0; property < computed.length; property++) {
      const name = computed.item(property);
      const value = computed.getPropertyValue(name);
      if (value !== "") copy.style.setProperty(name, value);
    }
  }
}

/**
 * Screen chrome is not card content: hide the export's own trigger and the other
 * header shortcuts (the [data-exporting] selector), and drop the focused
 * control's ring (outline or ring box-shadow) — focus is a screen state.
 * The card's fill also goes opaque here, where the index.css rule
 * ([data-exporting] .card-surface) cannot reach the detached clone: the theme's
 * surface color, via the --export-card token, even for the translucent
 * Native materials.
 */
function stripExportChrome(source: HTMLElement, clone: HTMLElement) {
  const originals = [source, ...source.querySelectorAll("*")];
  const copies = [clone, ...clone.querySelectorAll("*")];
  const focused = document.activeElement;
  for (let i = 0; i < originals.length && i < copies.length; i++) {
    const original = originals[i];
    const copy = copies[i] as HTMLElement;
    if (original.matches(EXPORT_HIDDEN_SELECTOR)) {
      copy.style.display = "none";
    }
    if (original === focused) {
      copy.style.outline = "none";
      copy.style.boxShadow = "none";
    }
  }
  for (const surface of Array.from(clone.querySelectorAll<HTMLElement>(".card-surface, .lifted-surface"))) {
    surface.style.backgroundColor = resolvedCssVar("--export-card");
  }
}

/** SVG data URL → `<img>` → canvas at the fixed export scale → `toDataURL`. */
async function rasterizePng(svg: string, width: number, height: number): Promise<string> {
  const url = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  const image = new Image();
  await new Promise<void>((resolve, reject) => {
    image.onload = () => resolve();
    image.onerror = () => reject(new Error("the card SVG failed to load"));
    image.src = url;
  });
  const canvas = document.createElement("canvas");
  canvas.width = Math.round(width * CARD_IMAGE_SCALE);
  canvas.height = Math.round(height * CARD_IMAGE_SCALE);
  const context = canvas.getContext("2d");
  if (!context) throw new Error("no 2d canvas");
  context.drawImage(image, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL("image/png");
}
