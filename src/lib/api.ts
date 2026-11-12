import { invoke } from "@tauri-apps/api/core";

/** Adresse du protocole `tsuki://` selon la plateforme. */
const ua = navigator.userAgent;
export const MEDIA = /Windows|Android/.test(ua) ? "http://tsuki.localhost/" : "tsuki://localhost/";

export interface VideoMeta {
  id: string;
  title?: string;
  uploadDate?: number;
  channelLogin?: string;
  channelName?: string;
  gameName?: string;
}

/** Message allégé : t = position dans la VOD, n = pseudo, l = login, c = couleur,
 *  b = badges [set, version], f = fragments [texte, id d'emote]. */
export interface Comment {
  t: number;
  n: string;
  l: string;
  c: string | null;
  b: [string, string][];
  f: [string, string | null][];
}

export interface ChatPayload {
  status: "loaded";
  path: string;
  generation: number;
  video: VideoMeta;
  startTime: number;
  comments: Comment[];
  emotes: string[];
  badges: [string, string][];
}

export interface ChatRefusal {
  status: "refused";
  level: 1 | 2 | 3;
  message: string;
  detail: string;
  path: string;
}

export type ChatResult = ChatPayload | ChatRefusal;

/** Vidéo de la bibliothèque. */
export interface Entry {
  id: string;
  path: string;
  chatPath: string | null;
  fileName: string;
  size: number;
  duration: number;
  title: string | null;
  channel: string | null;
  game: string | null;
  uploadDate: number | null;
  startTime: number;
  messageCount: number;
  addedAt: number;
  lastOpenedAt: number | null;
  position: number;
  hasThumbnail: boolean;
}

export interface LibraryItem extends Entry {
  exists: boolean;
}

export interface AddReport {
  added: Entry[];
  errors: { path: string; message: string }[];
}

export interface VideoInfo {
  id: number;
  entry: Entry;
  path: string;
  fileName: string;
  duration: number;
  hasAudio: boolean;
  wallclock: string | null;
  chat: ChatResult | null;
}

export const openVideo = (path: string) => invoke<VideoInfo>("open_video", { path });
export const openEntry = (id: string) => invoke<VideoInfo>("open_entry", { id });
export const libraryList = () => invoke<LibraryItem[]>("library_list");
export const libraryAdd = (paths: string[]) => invoke<AddReport>("library_add", { paths });
export const libraryRemove = (id: string) => invoke("library_remove", { id });
export const libraryRelocate = (id: string, path: string) => invoke<Entry>("library_relocate", { id, path });
export const setProgress = (id: string, position: number) =>
  invoke("set_progress", { id, position }).catch(() => {});
export const saveThumbnail = (id: string, data: string) =>
  invoke("save_thumbnail", { id, data }).catch((e) => log(`miniature : ${e}`));
export const loadChat = (videoId: number, path: string) =>
  invoke<ChatResult>("load_chat", { videoId, path });
export const startupFiles = () => invoke<string[]>("startup_files");
export const log = (message: string) => invoke("log", { message }).catch(() => {});

/** Miniature ; `v` force le rechargement après une nouvelle capture. */
export const thumbUrl = (id: string, v = 0) => `${MEDIA}t/${id}?v=${v}`;
export const playlistUrl = (v: VideoInfo) => `${MEDIA}v/${v.id}/index.m3u8`;
export const emoteUrl = (videoId: number, gen: number, id: string) =>
  `${MEDIA}e/${videoId}/${gen}/${encodeURIComponent(id)}`;
export const badgeUrl = (videoId: number, gen: number, set: string, version: string) =>
  `${MEDIA}b/${videoId}/${gen}/${encodeURIComponent(set)}/${encodeURIComponent(version)}`;
