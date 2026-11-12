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

export interface VideoInfo {
  id: number;
  path: string;
  fileName: string;
  duration: number;
  hasAudio: boolean;
  wallclock: string | null;
  chat: ChatResult | null;
}

export const openVideo = (path: string) => invoke<VideoInfo>("open_video", { path });
export const loadChat = (videoId: number, path: string) =>
  invoke<ChatResult>("load_chat", { videoId, path });
export const startupFile = () => invoke<string | null>("startup_file");
export const log = (message: string) => invoke("log", { message }).catch(() => {});

export const playlistUrl = (v: VideoInfo) => `${MEDIA}v/${v.id}/index.m3u8`;
export const emoteUrl = (videoId: number, gen: number, id: string) =>
  `${MEDIA}e/${videoId}/${gen}/${encodeURIComponent(id)}`;
export const badgeUrl = (videoId: number, gen: number, set: string, version: string) =>
  `${MEDIA}b/${videoId}/${gen}/${encodeURIComponent(set)}/${encodeURIComponent(version)}`;
