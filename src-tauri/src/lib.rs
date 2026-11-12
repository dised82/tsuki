//! Application Tsuki : commandes appelées par l'interface, bibliothèque des vidéos,
//! et protocole `tsuki://` qui sert la vidéo (morceau par morceau), les emotes,
//! les badges et les miniatures.

mod library;
mod media;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{Manager, State};
use tsuki_core::chat::{self, Chat, ChatError, VideoMeta};
use tsuki_core::ts_index::TsIndex;
use tsuki_core::validate::{self, Verdict};

use library::{Entry, Library};

/// Une vidéo ouverte pendant la session.
pub struct OpenVideo {
    pub path: PathBuf,
    pub index: Arc<TsIndex>,
    pub chat: Option<Arc<Chat>>,
    /// Incrémenté à chaque changement de chat (adresses des emotes uniques).
    pub chat_gen: u32,
    /// Entrée correspondante dans la bibliothèque.
    pub entry_id: String,
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
    entry: Entry,
    path: String,
    file_name: String,
    duration: f64,
    has_audio: bool,
    /// Heure réelle (UTC) du début de la vidéo, si connue.
    wallclock: Option<String>,
    /// Chat associé (enregistré dans la bibliothèque ou trouvé à côté de la vidéo).
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryItem {
    #[serde(flatten)]
    entry: Entry,
    /// Le fichier vidéo est-il toujours à son emplacement ?
    exists: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AddReport {
    added: Vec<Entry>,
    errors: Vec<AddError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AddError {
    path: String,
    message: String,
}

// --- Bibliothèque ------------------------------------------------------------

#[tauri::command]
fn library_list(lib: State<'_, Library>) -> Vec<LibraryItem> {
    let mut items: Vec<LibraryItem> = lib
        .list()
        .into_iter()
        .map(|entry| LibraryItem { exists: Path::new(&entry.path).is_file(), entry })
        .collect();
    // Les plus récemment regardées d'abord, puis les plus récemment ajoutées.
    items.sort_by_key(|i| std::cmp::Reverse(i.entry.last_opened_at.unwrap_or(0).max(i.entry.added_at)));
    items
}

/// Ajoute une ou plusieurs vidéos à la bibliothèque (sans les lire).
#[tauri::command]
async fn library_add(paths: Vec<String>, app: tauri::AppHandle) -> Result<AddReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let lib = app.state::<Library>();
        let mut report = AddReport { added: Vec::new(), errors: Vec::new() };
        for p in paths {
            match add_one(&lib, Path::new(&p)) {
                Ok(e) => report.added.push(e),
                Err(message) => report.errors.push(AddError { path: p, message }),
            }
        }
        report
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn library_remove(id: String, lib: State<'_, Library>) -> Result<(), String> {
    lib.remove(&id).map_err(|e| e.to_string())
}

/// Nouvel emplacement d'une vidéo déplacée : on vérifie que c'est bien le même fichier.
#[tauri::command]
async fn library_relocate(id: String, path: String, app: tauri::AppHandle) -> Result<Entry, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let lib = app.state::<Library>();
        let p = PathBuf::from(&path);
        let fp = library::fingerprint(&p).map_err(|e| format!("Lecture impossible : {e}"))?;
        if fp != id {
            return Err("Ce fichier n'est pas la même vidéo que celle de la bibliothèque.".into());
        }
        lib.update(&id, |e| {
            e.path = path.clone();
            e.file_name = file_name(&p);
            if e.chat_path.as_deref().is_none_or(|c| !Path::new(c).is_file()) {
                e.chat_path = sibling_chat(&p).map(|c| c.display().to_string());
            }
        })
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Vidéo absente de la bibliothèque.".into())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn set_progress(id: String, position: f64, lib: State<'_, Library>) -> Result<(), String> {
    lib.update(&id, |e| e.position = position.max(0.0)).map(|_| ()).map_err(|e| e.to_string())
}

/// Miniature capturée par l'interface pendant la lecture (JPEG en base64).
#[tauri::command]
fn save_thumbnail(id: String, data: String, lib: State<'_, Library>) -> Result<(), String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim_start_matches("data:image/jpeg;base64,"))
        .map_err(|e| e.to_string())?;
    std::fs::write(lib.thumb_path(&id), bytes).map_err(|e| e.to_string())?;
    lib.update(&id, |e| e.has_thumbnail = true).map(|_| ()).map_err(|e| e.to_string())
}

fn add_one(lib: &Library, path: &Path) -> Result<Entry, String> {
    if !path.is_file() {
        return Err("Fichier introuvable.".into());
    }
    let id = library::fingerprint(path).map_err(|e| format!("Lecture impossible : {e}"))?;
    let path_str = path.display().to_string();

    // Déjà connue (éventuellement déplacée) : on met simplement l'emplacement à jour.
    if let Some(mut e) = lib.get(&id) {
        e.path = path_str;
        e.file_name = file_name(path);
        if e.chat_path.as_deref().is_none_or(|c| !Path::new(c).is_file()) {
            attach_sibling_chat(&mut e, path, lib);
        }
        lib.upsert(e.clone()).map_err(|e| e.to_string())?;
        return Ok(e);
    }

    let index = lib.index(&id, path)?;
    let mut e = Entry {
        id,
        path: path_str,
        chat_path: None,
        file_name: file_name(path),
        size: index.file_size,
        duration: index.duration,
        title: None,
        channel: None,
        game: None,
        upload_date: None,
        start_time: 0.0,
        message_count: 0,
        added_at: library::now_ms(),
        last_opened_at: None,
        position: 0.0,
        has_thumbnail: false,
    };
    attach_sibling_chat(&mut e, path, lib);
    lib.upsert(e.clone()).map_err(|e| e.to_string())?;
    Ok(e)
}

/// Associe le `_chat.json` voisin s'il est valide et correspond à la vidéo.
fn attach_sibling_chat(e: &mut Entry, video: &Path, lib: &Library) {
    let Some(chat_path) = sibling_chat(video) else { return };
    let Ok(chat) = chat::read_file(&chat_path) else { return };
    let Ok(index) = lib.index(&e.id, video) else { return };
    if validate::check(&chat, &index, &file_name(video)).is_ok() {
        fill_meta(e, &chat, &chat_path);
    }
}

fn fill_meta(e: &mut Entry, chat: &Chat, chat_path: &Path) {
    let v = &chat.video;
    e.chat_path = Some(chat_path.display().to_string());
    e.title = v.title.clone();
    e.channel = v.channel_name.clone().or_else(|| v.channel_login.clone());
    e.game = v.game_name.clone();
    e.upload_date = v.upload_date;
    e.start_time = chat.start_time;
    e.message_count = chat.comments.len();
}

// --- Lecture -----------------------------------------------------------------

/// Ouvre un fichier (dialogue, glisser-déposer, ligne de commande) : il est ajouté
/// à la bibliothèque puis lu.
#[tauri::command]
async fn open_video(path: String, app: tauri::AppHandle, state: State<'_, AppState>) -> Result<VideoInfo, String> {
    let a = app.clone();
    let entry = tauri::async_runtime::spawn_blocking(move || add_one(&a.state::<Library>(), Path::new(&path)))
        .await
        .map_err(|e| e.to_string())??;
    open_entry(entry.id, app, state).await
}

/// Ouvre une vidéo de la bibliothèque.
#[tauri::command]
async fn open_entry(id: String, app: tauri::AppHandle, state: State<'_, AppState>) -> Result<VideoInfo, String> {
    let lib = app.state::<Library>();
    let entry = lib.get(&id).ok_or("Vidéo absente de la bibliothèque.")?;
    let video_path = PathBuf::from(&entry.path);
    if !video_path.is_file() {
        return Err("Fichier introuvable : la vidéo a été déplacée, renommée ou supprimée.".into());
    }
    let index = {
        let (a, id, p) = (app.clone(), id.clone(), video_path.clone());
        tauri::async_runtime::spawn_blocking(move || a.state::<Library>().index(&id, &p))
            .await
            .map_err(|e| e.to_string())??
    };
    let index = Arc::new(index);
    let vid = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    state.videos.lock().unwrap().insert(
        vid,
        OpenVideo {
            path: video_path.clone(),
            index: index.clone(),
            chat: None,
            chat_gen: 0,
            entry_id: id.clone(),
        },
    );
    let entry = lib
        .update(&id, |e| e.last_opened_at = Some(library::now_ms()))
        .map_err(|e| e.to_string())?
        .unwrap_or(entry);

    // Chat : celui enregistré dans la bibliothèque, sinon « <nom>_chat.json » à côté.
    let chat_path = entry
        .chat_path
        .as_ref()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| sibling_chat(&video_path));
    let chat = match chat_path {
        Some(p) => Some(load_chat_inner(vid, p, &state, &lib).await),
        None => None,
    };

    Ok(VideoInfo {
        id: vid,
        entry: lib.get(&id).unwrap_or(entry),
        path: video_path.display().to_string(),
        file_name: file_name(&video_path),
        duration: index.duration,
        has_audio: index.has_audio,
        wallclock: index.id3.wallclock.clone(),
        chat,
    })
}

#[tauri::command]
async fn load_chat(
    video_id: u32,
    path: String,
    state: State<'_, AppState>,
    lib: State<'_, Library>,
) -> Result<ChatResult, String> {
    Ok(load_chat_inner(video_id, PathBuf::from(path), &state, &lib).await)
}

/// Fichiers passés au lancement (`tsuki a.ts b.ts`) ou via `TSUKI_OPEN`.
#[tauri::command]
fn startup_files() -> Vec<String> {
    if let Ok(p) = std::env::var("TSUKI_OPEN") {
        return vec![p];
    }
    std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect()
}

/// Journal de l'interface, affiché dans le terminal (diagnostic).
#[tauri::command]
fn log(message: String) {
    eprintln!("[interface] {message}");
}

async fn load_chat_inner(video_id: u32, path: PathBuf, state: &State<'_, AppState>, lib: &Library) -> ChatResult {
    let refuse = |level: u8, message: &str, detail: String| {
        ChatResult::Refused(ChatRefusal {
            level,
            message: message.into(),
            detail,
            path: path.display().to_string(),
        })
    };

    let (index, video_name, entry_id) = {
        let videos = state.videos.lock().unwrap();
        match videos.get(&video_id) {
            Some(v) => (v.index.clone(), file_name(&v.path), v.entry_id.clone()),
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

    // Chat validé : retenu dans la bibliothèque avec les informations de la vidéo.
    let _ = lib.update(&entry_id, |e| fill_meta(e, &chat, &path));

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
            let dir = app.path().app_data_dir()?;
            app.manage(Library::load(dir));

            // En développement, la console de la fenêtre est recopiée dans le terminal.
            #[cfg(all(debug_assertions, target_os = "linux"))]
            {
                use webkit2gtk::{SettingsExt, WebViewExt};
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.with_webview(|wv| {
                        if let Some(s) = wv.inner().settings() {
                            s.set_enable_write_console_messages_to_stdout(true);
                        }
                    });
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_list,
            library_add,
            library_remove,
            library_relocate,
            set_progress,
            save_thumbnail,
            open_video,
            open_entry,
            load_chat,
            startup_files,
            log
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Tsuki");
}
