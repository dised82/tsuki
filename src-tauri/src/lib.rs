//! Application Tsuki : commandes appelées par l'interface et protocole `tsuki://`
//! qui sert la vidéo (morceau par morceau), les emotes et les badges.

mod media;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;
use tsuki_core::chat::{self, Chat, ChatError, VideoMeta};
use tsuki_core::ts_index::{self, TsIndex};
use tsuki_core::validate::{self, Verdict};

/// Une vidéo ouverte pendant la session.
pub struct OpenVideo {
    pub path: PathBuf,
    pub index: Arc<TsIndex>,
    pub chat: Option<Arc<Chat>>,
    /// Incrémenté à chaque changement de chat (adresses des emotes uniques).
    pub chat_gen: u32,
}

#[derive(Default)]
pub struct AppState {
    pub videos: Mutex<HashMap<u32, OpenVideo>>,
    next_id: AtomicU32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VideoInfo {
    id: u32,
    path: String,
    file_name: String,
    duration: f64,
    has_audio: bool,
    /// Heure réelle (UTC) du début de la vidéo, si connue.
    wallclock: Option<String>,
    /// Chat trouvé automatiquement à côté de la vidéo.
    chat: Option<ChatResult>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum ChatResult {
    Loaded(ChatPayload),
    Refused(ChatRefusal),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatRefusal {
    /// 1 = pas du JSON, 2 = pas un chat Xtra, 3 = ne correspond pas à la vidéo.
    level: u8,
    message: String,
    detail: String,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatPayload {
    path: String,
    generation: u32,
    video: VideoMeta,
    start_time: f64,
    comments: Vec<CompactComment>,
    emotes: Vec<String>,
    badges: Vec<(String, String)>,
}

/// Message allégé envoyé à l'interface.
#[derive(Serialize)]
struct CompactComment {
    /// Position dans la VOD (s).
    t: f64,
    /// Pseudo affiché, identifiant de connexion, couleur.
    n: String,
    l: String,
    c: Option<String>,
    /// Badges `(set, version)`.
    b: Vec<(String, String)>,
    /// Fragments `(texte, emote éventuelle)`.
    f: Vec<(String, Option<String>)>,
}

#[tauri::command]
async fn open_video(path: String, state: State<'_, AppState>) -> Result<VideoInfo, String> {
    let video_path = PathBuf::from(&path);
    let index = {
        let p = video_path.clone();
        tauri::async_runtime::spawn_blocking(move || ts_index::index_file(&p))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?
    };
    let index = Arc::new(index);
    let id = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    state.videos.lock().unwrap().insert(
        id,
        OpenVideo { path: video_path.clone(), index: index.clone(), chat: None, chat_gen: 0 },
    );

    // Chat rangé à côté par Xtra : « <nom>_chat.json ».
    let sibling = sibling_chat(&video_path);
    let chat = match sibling {
        Some(p) => Some(load_chat_inner(id, p, &state).await),
        None => None,
    };

    Ok(VideoInfo {
        id,
        path,
        file_name: file_name(&video_path),
        duration: index.duration,
        has_audio: index.has_audio,
        wallclock: index.id3.wallclock.clone(),
        chat,
    })
}

#[tauri::command]
async fn load_chat(video_id: u32, path: String, state: State<'_, AppState>) -> Result<ChatResult, String> {
    Ok(load_chat_inner(video_id, PathBuf::from(path), &state).await)
}

/// Chemin passé au lancement (`tsuki fichier.ts`) ou via `TSUKI_OPEN`.
#[tauri::command]
fn startup_file() -> Option<String> {
    std::env::var("TSUKI_OPEN")
        .ok()
        .or_else(|| std::env::args().skip(1).find(|a| !a.starts_with('-')))
}

/// Journal de l'interface, affiché dans le terminal (diagnostic).
#[tauri::command]
fn log(message: String) {
    eprintln!("[interface] {message}");
}

async fn load_chat_inner(video_id: u32, path: PathBuf, state: &State<'_, AppState>) -> ChatResult {
    let refuse = |level: u8, message: &str, detail: String| {
        ChatResult::Refused(ChatRefusal {
            level,
            message: message.into(),
            detail,
            path: path.display().to_string(),
        })
    };

    let (index, video_name) = {
        let videos = state.videos.lock().unwrap();
        match videos.get(&video_id) {
            Some(v) => (v.index.clone(), file_name(&v.path)),
            None => return refuse(3, "Aucune vidéo en cours de lecture.", String::new()),
        }
    };

    let p = path.clone();
    let parsed = tauri::async_runtime::spawn_blocking(move || chat::read_file(&p)).await;
    let chat = match parsed {
        Ok(Ok(c)) => c,
        Ok(Err(ChatError::Io(e))) => return refuse(1, "Impossible de lire ce fichier.", e.to_string()),
        Ok(Err(ChatError::NotJson(d))) => {
            return refuse(1, "Ce fichier n'est pas un fichier JSON valide.", d)
        }
        Ok(Err(ChatError::NotXtra(d))) => return refuse(2, "Ce fichier JSON n'est pas un chat Xtra.", d),
        Err(e) => return refuse(1, "Impossible de lire ce fichier.", e.to_string()),
    };

    match validate::check(&chat, &index, &video_name) {
        Verdict::Match { .. } => {}
        Verdict::Mismatch { reason } => {
            return refuse(3, "Ce chat ne correspond pas à la vidéo en cours de lecture.", capitalize(&reason))
        }
        Verdict::Unverifiable { reason } => {
            return refuse(
                3,
                "Impossible de vérifier que ce chat correspond à la vidéo.",
                capitalize(&reason),
            )
        }
    }

    let chat = Arc::new(chat);
    let generation = {
        let mut videos = state.videos.lock().unwrap();
        let Some(v) = videos.get_mut(&video_id) else {
            return refuse(3, "La vidéo a été fermée.", String::new());
        };
        v.chat = Some(chat.clone());
        v.chat_gen += 1;
        v.chat_gen
    };

    ChatResult::Loaded(ChatPayload {
        path: path.display().to_string(),
        generation,
        video: chat.video.clone(),
        start_time: chat.start_time,
        comments: chat
            .comments
            .iter()
            .map(|c| {
                let who = c.commenter.as_ref();
                let login = who.and_then(|w| w.login.clone()).unwrap_or_default();
                CompactComment {
                    t: c.offset,
                    n: who.and_then(|w| w.display_name.clone()).unwrap_or_else(|| login.clone()),
                    l: login,
                    c: c.message.color.clone().filter(|s| !s.is_empty()),
                    b: c
                        .message
                        .badges
                        .iter()
                        .filter(|b| !b.set_id.is_empty())
                        .map(|b| (b.set_id.clone(), b.version.clone()))
                        .collect(),
                    f: c
                        .message
                        .fragments
                        .iter()
                        .map(|f| (f.text.clone(), f.emote.as_ref().map(|e| e.id.clone())))
                        .collect(),
                }
            })
            .collect(),
        emotes: chat.emotes.keys().cloned().collect(),
        badges: chat.badges.keys().cloned().collect(),
    })
}

fn sibling_chat(video: &Path) -> Option<PathBuf> {
    let stem = video.file_stem()?.to_str()?;
    let candidate = video.with_file_name(format!("{stem}_chat.json"));
    candidate.is_file().then_some(candidate)
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .register_asynchronous_uri_scheme_protocol("tsuki", media::handle)
        .setup(|app| {
            // En développement, la console de la fenêtre est recopiée dans le terminal.
            #[cfg(all(debug_assertions, target_os = "linux"))]
            {
                use tauri::Manager;
                use webkit2gtk::{SettingsExt, WebViewExt};
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.with_webview(|wv| {
                        if let Some(s) = wv.inner().settings() {
                            s.set_enable_write_console_messages_to_stdout(true);
                        }
                    });
                }
            }
            let _ = app;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![open_video, load_chat, startup_file, log])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Tsuki");
}
