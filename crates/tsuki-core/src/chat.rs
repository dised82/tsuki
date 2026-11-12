//! Lecture des fichiers de chat `*_chat.json` produits par Xtra.
//!
//! Particularité du format : Xtra écrit le fichier par morceaux, si bien que les clés
//! `comments`, `twitchEmotes` et `twitchBadges` apparaissent **plusieurs fois** à la
//! racine. Un lecteur JSON classique ne garde que la dernière occurrence ; ici on lit
//! la racine entrée par entrée et on additionne tout.

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("lecture du fichier impossible : {0}")]
    Io(#[from] std::io::Error),
    /// Niveau 1 : le fichier n'est pas du JSON valide.
    #[error("ce fichier n'est pas un fichier JSON valide ({0})")]
    NotJson(String),
    /// Niveau 2 : JSON valide, mais pas un chat Xtra.
    #[error("ce fichier JSON n'est pas un chat Xtra ({0})")]
    NotXtra(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMeta {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Début du stream, en millisecondes Unix.
    #[serde(default)]
    pub upload_date: Option<i64>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub channel_login: Option<String>,
    #[serde(default)]
    pub channel_name: Option<String>,
    #[serde(default)]
    pub game_id: Option<String>,
    #[serde(default)]
    pub game_slug: Option<String>,
    #[serde(default)]
    pub game_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commenter {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub login: Option<String>,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmoteRef {
    #[serde(rename = "emoteID")]
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fragment {
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emote: Option<EmoteRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BadgeRef {
    #[serde(rename = "setID", default)]
    pub set_id: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub fragments: Vec<Fragment>,
    #[serde(rename = "userBadges", default)]
    pub badges: Vec<BadgeRef>,
    #[serde(rename = "userColor", default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    /// Position du message dans la VOD, en secondes.
    #[serde(rename = "contentOffsetSeconds")]
    pub offset: f64,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub commenter: Option<Commenter>,
    pub message: Message,
}

/// Image encodée en base64 dans le JSON (PNG ou GIF animé).
#[derive(Debug, Clone)]
pub struct Image {
    pub base64: String,
}

impl Image {
    pub fn mime(&self) -> &'static str {
        match self.base64.get(..6) {
            Some("R0lGOD") => "image/gif",
            Some("iVBORw") => "image/png",
            Some(s) if s.starts_with("/9j/") => "image/jpeg",
            Some("UklGRg") => "image/webp",
            _ => "application/octet-stream",
        }
    }

    /// Décode l'image. Xtra omet le remplissage `=` final : on l'accepte, avec ou sans,
    /// et en variante « URL » (`-` et `_`) au cas où.
    pub fn bytes(&self) -> Option<Vec<u8>> {
        use base64::alphabet::{STANDARD, URL_SAFE};
        use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
        use base64::Engine;
        let config = GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent);
        let data = self.base64.trim();
        let alphabet = if data.contains(['-', '_']) { &URL_SAFE } else { &STANDARD };
        GeneralPurpose::new(alphabet, config).decode(data).ok()
    }
}

#[derive(Debug, Clone)]
pub struct Chat {
    pub video: VideoMeta,
    /// Début du passage téléchargé, en secondes depuis le début de la VOD.
    pub start_time: f64,
    /// Messages triés par position, sans doublons.
    pub comments: Vec<Comment>,
    /// Emotes par identifiant.
    pub emotes: HashMap<String, Image>,
    /// Badges par `(set, version)`.
    pub badges: HashMap<(String, String), Image>,
    /// Clés racine inconnues (pour repérer une évolution du format d'Xtra).
    pub unknown_keys: Vec<String>,
}

impl Chat {
    /// Index du premier message dont la position dépasse `vod_time`.
    /// Les messages `[..i]` sont ceux déjà « postés » à cet instant.
    pub fn upto(&self, vod_time: f64) -> usize {
        self.comments.partition_point(|c| c.offset <= vod_time)
    }
}

pub fn read_file(path: &Path) -> Result<Chat, ChatError> {
    read(BufReader::with_capacity(1 << 20, File::open(path)?))
}

pub fn read<R: Read>(reader: R) -> Result<Chat, ChatError> {
    let mut de = serde_json::Deserializer::from_reader(reader);
    let raw = RawChat::deserialize(&mut de).map_err(classify)?;
    de.end().map_err(classify)?;

    let video = raw.video.ok_or_else(|| ChatError::NotXtra("bloc « video » absent".into()))?;
    if !raw.saw_comments {
        return Err(ChatError::NotXtra("liste « comments » absente".into()));
    }

    let mut comments = raw.comments;
    comments.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    let mut seen = std::collections::HashSet::with_capacity(comments.len());
    comments.retain(|c| seen.insert(c.id.clone()));

    Ok(Chat {
        video,
        start_time: raw.start_time.unwrap_or(0.0),
        comments,
        emotes: raw.emotes.into_iter().map(|e| (e.id, Image { base64: e.data })).collect(),
        badges: raw
            .badges
            .into_iter()
            .map(|b| ((b.set_id, b.version), Image { base64: b.data }))
            .collect(),
        unknown_keys: raw.unknown_keys,
    })
}

fn classify(e: serde_json::Error) -> ChatError {
    use serde_json::error::Category;
    match e.classify() {
        Category::Io => ChatError::Io(e.into()),
        Category::Syntax | Category::Eof => ChatError::NotJson(e.to_string()),
        Category::Data => ChatError::NotXtra(e.to_string()),
    }
}

#[derive(Deserialize)]
struct RawEmote {
    id: String,
    data: String,
}

#[derive(Deserialize)]
struct RawBadge {
    #[serde(rename = "setId")]
    set_id: String,
    version: String,
    data: String,
}

#[derive(Default)]
struct RawChat {
    video: Option<VideoMeta>,
    start_time: Option<f64>,
    saw_comments: bool,
    comments: Vec<Comment>,
    emotes: Vec<RawEmote>,
    badges: Vec<RawBadge>,
    unknown_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for RawChat {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = RawChat;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("un objet JSON de chat Xtra")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<RawChat, A::Error> {
                let mut raw = RawChat::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "video" => raw.video = Some(map.next_value()?),
                        "startTime" => raw.start_time = Some(map.next_value()?),
                        "comments" => {
                            raw.saw_comments = true;
                            raw.comments.extend(map.next_value::<Vec<Comment>>()?);
                        }
                        "twitchEmotes" => raw.emotes.extend(map.next_value::<Vec<RawEmote>>()?),
                        "twitchBadges" => raw.badges.extend(map.next_value::<Vec<RawBadge>>()?),
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                            if !raw.unknown_keys.contains(&key) {
                                raw.unknown_keys.push(key);
                            }
                        }
                    }
                }
                Ok(raw)
            }
        }
        // Pas de `map_err` ici : on garde la catégorie d'erreur de serde_json
        // (syntaxe → niveau 1, contenu → niveau 2).
        d.deserialize_map(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUPLICATED: &str = r#"{
        "video": {"id": "42", "title": "Test", "uploadDate": 1793995223000},
        "startTime": 10,
        "comments": [{"id": "b", "contentOffsetSeconds": 20, "message": {"fragments": [{"text": "deux"}], "userBadges": []}}],
        "twitchEmotes": [{"id": "1", "data": "iVBORw0KGgo="}],
        "comments": [
            {"id": "a", "contentOffsetSeconds": 5, "message": {"fragments": [{"text": "un"}], "userBadges": []}},
            {"id": "b", "contentOffsetSeconds": 20, "message": {"fragments": [{"text": "deux"}], "userBadges": []}}
        ],
        "twitchEmotes": [{"id": "2", "data": "R0lGODlh"}],
        "futureKey": {"x": 1}
    }"#;

    #[test]
    fn merges_repeated_keys_sorts_and_dedupes() {
        let chat = read(DUPLICATED.as_bytes()).unwrap();
        let ids: Vec<_> = chat.comments.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(chat.start_time, 10.0);
        assert_eq!(chat.emotes.len(), 2);
        assert_eq!(chat.emotes["1"].mime(), "image/png");
        assert_eq!(chat.emotes["2"].mime(), "image/gif");
        assert_eq!(chat.unknown_keys, ["futureKey"]);
        assert_eq!(chat.upto(4.0), 0);
        assert_eq!(chat.upto(5.0), 1);
        assert_eq!(chat.upto(100.0), 2);
    }

    #[test]
    fn decodes_unpadded_base64() {
        // « ab » encodé = « YWI= » ; Xtra écrit « YWI ».
        for data in ["YWI", "YWI="] {
            assert_eq!(Image { base64: data.into() }.bytes().as_deref(), Some(&b"ab"[..]));
        }
    }

    #[test]
    fn error_levels() {
        assert!(matches!(read(&b"bonjour"[..]), Err(ChatError::NotJson(_))));
        assert!(matches!(read(&DUPLICATED.as_bytes()[..200]), Err(ChatError::NotJson(_))));
        assert!(matches!(read(&b"[1, 2]"[..]), Err(ChatError::NotXtra(_))));
        assert!(matches!(read(&br#"{"a": 1}"#[..]), Err(ChatError::NotXtra(_))));
        assert!(matches!(read(&br#"{"video": {"id": "1"}}"#[..]), Err(ChatError::NotXtra(_))));
        let bad = br#"{"video": {"id": "1"}, "comments": [{"id": "x"}]}"#;
        assert!(matches!(read(&bad[..]), Err(ChatError::NotXtra(_))));
    }
}
