/** 4805.2 → "1:20:05" ; 65 → "1:05" */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

export function longDate(ms: number): string {
  return new Date(ms).toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" });
}

/** Couleurs par défaut de Twitch pour les utilisateurs sans couleur choisie. */
const DEFAULT_COLORS = [
  "#FF0000", "#0000FF", "#008000", "#B22222", "#FF7F50", "#9ACD32", "#FF4500", "#2E8B57",
  "#DAA520", "#D2691E", "#5F9EA0", "#1E90FF", "#FF69B4", "#8A2BE2", "#00FF7F",
];

const cache = new Map<string, string>();

/** Couleur du pseudo, éclaircie si elle est illisible sur fond sombre. */
export function nameColor(color: string | null, login: string): string {
  const key = `${color}|${login}`;
  const hit = cache.get(key);
  if (hit) return hit;
  let base = color;
  if (!base) {
    let h = 0;
    for (const ch of login) h = (h * 31 + ch.charCodeAt(0)) | 0;
    base = DEFAULT_COLORS[Math.abs(h) % DEFAULT_COLORS.length];
  }
  const out = readable(base);
  cache.set(key, out);
  return out;
}

function readable(hex: string): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return "#bf94ff";
  const n = parseInt(m[1], 16);
  let [h, s, l] = rgbToHsl((n >> 16) & 255, (n >> 8) & 255, n & 255);
  // Fond ≈ #18181b : on garantit une luminosité minimale (et un bleu pur moins saturé).
  if (l < 0.55) l = 0.55 + (0.55 - l) * 0.25;
  if (s > 0.9 && h > 200 && h < 260) s = 0.85;
  return `hsl(${h.toFixed(0)} ${(s * 100).toFixed(0)}% ${(l * 100).toFixed(0)}%)`;
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, s, l];
}

/** Stockage local tolérant (peut être indisponible). */
export function stored<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(`tsuki.${key}`);
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}
export function store(key: string, value: unknown) {
  try {
    localStorage.setItem(`tsuki.${key}`, JSON.stringify(value));
  } catch {
    /* ignoré */
  }
}
