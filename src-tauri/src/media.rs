//! Protocole `tsuki://` (sous Windows et Android : `http://tsuki.localhost/`).
//!
//! | Adresse                              | Contenu                                  |
//! |--------------------------------------|------------------------------------------|
//! | `/v/{vidéo}/index.m3u8`              | playlist HLS générée depuis l'index      |
//! | `/v/{vidéo}/{n}.ts`                  | morceau n, lu directement dans le `.ts`  |
//! | `/e/{vidéo}/{génération}/{emote}`    | image d'emote (décodée du JSON)          |
//! | `/b/{vidéo}/{génération}/{set}/{v}`  | image de badge                           |
//! | `/t/{entrée}`                        | miniature d'une vidéo de la bibliothèque |

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder};

use crate::library::{safe_id, Library};
use crate::AppState;

pub fn handle<R: tauri::Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    // La lecture disque se fait hors du fil principal de l'interface.
    std::thread::spawn(move || {
        let response = route(app.state::<AppState>().inner(), app.state::<Library>().inner(), request.uri().path())
            .unwrap_or_else(|(status, msg)| reply(status, "text/plain; charset=utf-8", msg.into_bytes(), false));
        responder.respond(response);
    });
}

type Fail = (StatusCode, String);

fn route(state: &AppState, lib: &Library, path: &str) -> Result<Response<Vec<u8>>, Fail> {
    let parts: Vec<String> = path.trim_start_matches('/').split('/').map(percent_decode).collect();
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    let not_found = || (StatusCode::NOT_FOUND, format!("introuvable : {path}"));
    let id = |s: &str| s.parse::<u32>().map_err(|_| not_found());

    match parts.as_slice() {
        ["v", vid, "index.m3u8"] => {
            let index = video(state, id(vid)?)?.0;
            let body = index.to_m3u8_split(|i| format!("{i}.ts"));
            Ok(reply(StatusCode::OK, "application/vnd.apple.mpegurl", body.into_bytes(), false))
        }
        ["v", vid, seg] => {
            let n: usize = seg.strip_suffix(".ts").and_then(|s| s.parse().ok()).ok_or_else(not_found)?;
            let (index, path) = video(state, id(vid)?)?;
            let s = index.segments.get(n).ok_or_else(not_found)?;
            let mut buf = vec![0u8; s.len as usize];
            let io = |e: std::io::Error| (StatusCode::INTERNAL_SERVER_ERROR, format!("lecture : {e}"));
            let mut f = File::open(&path).map_err(io)?;
            f.seek(SeekFrom::Start(s.offset)).map_err(io)?;
            f.read_exact(&mut buf).map_err(io)?;
            Ok(reply(StatusCode::OK, "video/mp2t", buf, false))
        }
        ["e", vid, _gen, emote] => image(state, id(vid)?, |c| c.emotes.get(*emote)),
        ["b", vid, _gen, set, version] => {
            image(state, id(vid)?, |c| c.badges.get(&(set.to_string(), version.to_string())))
        }
        ["t", entry] => {
            let bytes = std::fs::read(lib.thumb_path(&safe_id(entry))).map_err(|_| not_found())?;
            Ok(reply(StatusCode::OK, "image/jpeg", bytes, false))
        }
        _ => Err(not_found()),
    }
}

fn video(state: &AppState, id: u32) -> Result<(std::sync::Arc<tsuki_core::ts_index::TsIndex>, std::path::PathBuf), Fail> {
    let videos = state.videos.lock().unwrap();
    let v = videos.get(&id).ok_or((StatusCode::NOT_FOUND, format!("vidéo {id} non ouverte")))?;
    Ok((v.index.clone(), v.path.clone()))
}

fn image(
    state: &AppState,
    id: u32,
    pick: impl Fn(&tsuki_core::chat::Chat) -> Option<&tsuki_core::chat::Image>,
) -> Result<Response<Vec<u8>>, Fail> {
    let chat = state.videos.lock().unwrap().get(&id).and_then(|v| v.chat.clone());
    let missing = || (StatusCode::NOT_FOUND, "image absente du chat".to_string());
    let img = chat.as_deref().and_then(pick).ok_or_else(missing)?;
    let bytes = img.bytes().ok_or_else(missing)?;
    Ok(reply(StatusCode::OK, img.mime(), bytes, true))
}

fn reply(status: StatusCode, mime: &str, body: Vec<u8>, cache: bool) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        // L'interface (tauri://localhost) et ce protocole sont deux origines distinctes.
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, if cache { "max-age=31536000, immutable" } else { "no-store" })
        .body(body)
        .unwrap()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
