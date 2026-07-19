// Shared helpers for the ReClass-style inspector: structured clipboard payloads
// (mirroring the egui `ClipboardPayload`), per-kind coloring, and the hex-view
// byte interpretations. Kept out of the components so both panel and row share them.
import { parseAddr } from "./api";

// ---- selection ----------------------------------------------------------

/** The single selected field the toolbar / keyboard shortcuts act on. */
export interface FieldSel {
  /** Path key (dotted field indices) used to highlight the row. */
  key: string;
  /** The class that directly owns the field (a pointer target for nested rows). */
  ownerClass: string;
  index: number;
  address: number;
  size: number;
  kind: string;
  name: string;
  metadata: string | null;
}

// ---- clipboard (webview navigator.clipboard, graceful on failure) -------

export async function clipWrite(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export async function clipRead(): Promise<string | null> {
  try {
    return await navigator.clipboard.readText();
  } catch {
    return null;
  }
}

// ---- structured payloads (mirror egui's ClipboardPayload) ---------------

export interface ClipField {
  name: string;
  offset: number;
  kind: string;
  metadata: string | null;
}
export type ClipPayload =
  | { t: "fields"; fields: ClipField[] }
  | { t: "value"; bytes: number[] }
  | { t: "address"; address: number };

/** Interprets pasted text: a structured payload, else a bare hex address. */
export function parsePayload(
  text: string,
): ClipPayload | { t: "addr"; address: number } | null {
  try {
    const p = JSON.parse(text);
    if (p && (p.t === "fields" || p.t === "value" || p.t === "address")) {
      return p as ClipPayload;
    }
  } catch {
    // not JSON — fall through to the least-destructive address interpretation
  }
  const a = parseAddr(text.trim());
  return a !== null ? { t: "addr", address: a } : null;
}

// ---- per-kind text color (egui field palette) ---------------------------

export function kindColor(kind: string): string {
  if (kind === "Bool") return "text-warn";
  if (/^U(8|16|32|64)$/.test(kind)) return "text-success";
  if (/^I(8|16|32|64)$/.test(kind)) return "text-accent";
  if (kind === "F32" || kind === "F64") return "text-danger";
  if (kind === "Ptr") return "text-[#c9955f]";
  if (kind === "StrPtr") return "text-danger";
  if (kind.startsWith("Vec") || kind.startsWith("Mat")) return "text-danger";
  if (kind.startsWith("Hex")) return "text-faint";
  return "text-muted";
}

// ---- hex-view interpretations for an Unk field's raw bytes ---------------

export function hexPairs(bytes: number[]): string {
  return bytes.map((b) => (b & 0xff).toString(16).toUpperCase().padStart(2, "0")).join(" ");
}

/** Little-endian signed integer over up to 8 bytes. */
export function asInt(bytes: number[]): string {
  let v = 0n;
  for (let i = bytes.length - 1; i >= 0; i--) v = (v << 8n) | BigInt(bytes[i] & 0xff);
  const bits = BigInt(bytes.length * 8);
  if (v & (1n << (bits - 1n))) v -= 1n << bits;
  return v.toString();
}

/** A "plausible" little-endian float (f64 then f32), or null if it looks like noise. */
export function asFloat(bytes: number[]): string | null {
  const dv = new DataView(new Uint8Array(bytes).buffer);
  const ok = (f: number) => Number.isFinite(f) && Math.abs(f) >= 1e-6 && Math.abs(f) <= 1e9;
  if (bytes.length >= 8) {
    const f = dv.getFloat64(0, true);
    if (ok(f)) return fmtFloat(f);
  }
  if (bytes.length >= 4) {
    const f = dv.getFloat32(0, true);
    if (ok(f)) return fmtFloat(f);
  }
  return null;
}

function fmtFloat(v: number): string {
  return Number.isInteger(v) ? v.toFixed(1) : String(+v.toPrecision(6));
}
