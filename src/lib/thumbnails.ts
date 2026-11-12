// Miniatures des vidéos jamais lues : un lecteur invisible ouvre la vidéo, se place
// vers 10 % de sa durée, capture une image puis se ferme. Une vidéo à la fois.
import Hls from "hls.js";
import { closeVideo, log, openQuiet, playlistUrl, saveThumbnail } from "./api";

const TIMEOUT_MS = 45_000;
const queue: string[] = [];
let running = false;
let onDone: (id: string) => void = () => {};

/** Appelé après chaque miniature enregistrée (pour rafraîchir la bibliothèque). */
export function onThumbnailSaved(cb: (id: string) => void) {
  onDone = cb;
}

export function queueThumbnails(ids: string[]) {
  for (const id of ids) if (!queue.includes(id)) queue.push(id);
  if (!running) void run();
}

async function run() {
  running = true;
  while (queue.length > 0) {
    const id = queue.shift()!;
    try {
      if (await capture(id)) onDone(id);
    } catch (e) {
      log(`miniature ${id} : ${e}`);
    }
  }
  running = false;
}

async function capture(id: string): Promise<boolean> {
  const info = await openQuiet(id);
  const at = Math.min(Math.max(info.duration * 0.1, 3), 300);
  const el = document.createElement("video");
  el.muted = true;
  el.playsInline = true;
  // Taille normale mais derrière l'interface : WebKit ne décode pas une vidéo minuscule.
  el.style.cssText = "position:fixed;left:0;top:0;width:320px;height:180px;opacity:0.01;z-index:-1;pointer-events:none";
  document.body.appendChild(el);
  const hls = new Hls({ startPosition: at, maxBufferLength: 8, enableWorker: true });
  const c = document.createElement("canvas");
  try {
    let failed: string | null = null;
    hls.on(Hls.Events.ERROR, (_e, d) => d.fatal && (failed = d.details));
    hls.loadSource(playlistUrl(info));
    hls.attachMedia(el);
    el.play().catch(() => {});

    const deadline = Date.now() + TIMEOUT_MS;
    let frames = 0;
    while (Date.now() < deadline) {
      if (failed) throw new Error(failed);
      await nextFrame(el);
      if (!el.videoWidth || el.currentTime < at - 0.5) continue;
      if (++frames < 3) continue; // quelques images pour laisser le décodeur se stabiliser
      c.width = 480;
      c.height = Math.round((480 * el.videoHeight) / el.videoWidth);
      const ctx = c.getContext("2d", { willReadFrequently: true })!;
      ctx.drawImage(el, 0, 0, c.width, c.height);
      if (brightness(ctx, c.width, c.height) > 12) {
        await saveThumbnail(id, c.toDataURL("image/jpeg", 0.82));
        return true;
      }
      // Image noire (fondu, écran d'attente ou image pas encore peinte) : un peu plus loin.
      if (frames > 40) {
        el.currentTime = Math.min(el.currentTime + 20, info.duration - 1);
        frames = 0;
      }
    }
    throw new Error("délai dépassé");
  } finally {
    hls.destroy();
    el.remove();
    closeVideo(info.id);
  }
}

/** Attend l'image suivante réellement affichée (ou ~100 ms sans requestVideoFrameCallback). */
function nextFrame(el: HTMLVideoElement): Promise<void> {
  return new Promise((resolve) => {
    const t = setTimeout(resolve, 250);
    const v = el as HTMLVideoElement & { requestVideoFrameCallback?: (cb: () => void) => number };
    if (v.requestVideoFrameCallback) v.requestVideoFrameCallback(() => { clearTimeout(t); resolve(); });
  });
}

/** Luminosité moyenne (0–255) sur une grille de points. */
function brightness(ctx: CanvasRenderingContext2D, w: number, h: number): number {
  const { data } = ctx.getImageData(0, 0, w, h);
  let sum = 0, n = 0;
  for (let i = 0; i < data.length; i += 4 * 97) {
    sum += data[i] * 0.3 + data[i + 1] * 0.59 + data[i + 2] * 0.11;
    n++;
  }
  return n ? sum / n : 0;
}
