//! Bibliothèque : les vidéos déjà ouvertes, leur emplacement, leur chat, la position
//! de lecture et une miniature. Propre à chaque appareil, enregistrée dans le dossier
//! de données de l'application (`~/.local/share/app.tsuki.player/` sous Linux).
//!
//! Chaque vidéo est identifiée par une **empreinte** (taille + début et fin du fichier) :
//! une vidéo déplacée ou renommée est reconnue, et « Relocaliser » vérifie qu'on
//! désigne bien le même fichier.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tsuki_core::ts_index::{self, TsIndex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Empreinte du fichier vidéo.
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub chat_path: Option<String>,
    pub file_name: String,
    pub size: u64,
    pub duration: f64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub game: Option<String>,
    /// Début du stream (ms Unix).
    #[serde(default)]
    pub upload_date: Option<i64>,
    /// Début du passage dans la VOD (s).
    #[serde(default)]
    pub start_time: f64,
    #[serde(default)]
    pub message_count: usize,
    pub added_at: i64,
    #[serde(default)]
    pub last_opened_at: Option<i64>,
    /// Position de lecture (s), pour la reprise.
    #[serde(default)]
    pub position: f64,
    #[serde(default)]
    pub has_thumbnail: bool,
}

pub struct Library {
    dir: PathBuf,
    entries: Mutex<Vec<Entry>>,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Library {
    pub fn load(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(dir.join("thumbs"));
        let _ = fs::create_dir_all(dir.join("index"));
        let entries = fs::read(dir.join("library.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Entry>>(&b).ok())
            .unwrap_or_default();
        Library { dir, entries: Mutex::new(entries) }
    }

    pub fn list(&self) -> Vec<Entry> {
        self.entries.lock().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Option<Entry> {
        self.entries.lock().unwrap().iter().find(|e| e.id == id).cloned()
    }

    /// Ajoute ou remplace une entrée, puis enregistre.
    pub fn upsert(&self, entry: Entry) -> io::Result<()> {
        let mut list = self.entries.lock().unwrap();
        match list.iter_mut().find(|e| e.id == entry.id) {
            Some(e) => *e = entry,
            None => list.push(entry),
        }
        self.save(&list)
    }

    /// Modifie une entrée existante, puis enregistre.
    pub fn update(&self, id: &str, f: impl FnOnce(&mut Entry)) -> io::Result<Option<Entry>> {
        let mut list = self.entries.lock().unwrap();
        let Some(e) = list.iter_mut().find(|e| e.id == id) else { return Ok(None) };
        f(e);
        let out = e.clone();
        self.save(&list)?;
        Ok(Some(out))
    }

    pub fn remove(&self, id: &str) -> io::Result<()> {
        let mut list = self.entries.lock().unwrap();
        list.retain(|e| e.id != id);
        let _ = fs::remove_file(self.thumb_path(id));
        let _ = fs::remove_file(self.index_path(id));
        self.save(&list)
    }

    /// Écriture atomique : fichier temporaire puis renommage.
    fn save(&self, list: &[Entry]) -> io::Result<()> {
        let tmp = self.dir.join("library.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(list).map_err(io::Error::other)?)?;
        fs::rename(tmp, self.dir.join("library.json"))
    }

    pub fn thumb_path(&self, id: &str) -> PathBuf {
        self.dir.join("thumbs").join(format!("{}.jpg", safe_id(id)))
    }

    fn index_path(&self, id: &str) -> PathBuf {
        self.dir.join("index").join(format!("{}.json", safe_id(id)))
    }

    /// Table de navigation de la vidéo, mise en cache : la réouverture est immédiate.
    pub fn index(&self, id: &str, video: &Path) -> Result<TsIndex, String> {
        let cache = self.index_path(id);
        if let Some(idx) = fs::read(&cache).ok().and_then(|b| serde_json::from_slice::<TsIndex>(&b).ok()) {
            return Ok(idx);
        }
        let idx = ts_index::index_file(video).map_err(|e| e.to_string())?;
        if let Ok(b) = serde_json::to_vec(&idx) {
            let _ = fs::write(cache, b);
        }
        Ok(idx)
    }
}

/// N'autorise que des caractères hexadécimaux dans les noms de fichiers dérivés d'un id.
pub fn safe_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_hexdigit()).collect()
}

/// Empreinte : taille + FNV-1a 64 bits du premier et du dernier Mo du fichier.
pub fn fingerprint(path: &Path) -> io::Result<String> {
    const CHUNK: u64 = 1 << 20;
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |buf: &[u8]| {
        for &b in buf {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    let mut buf = vec![0u8; CHUNK.min(size) as usize];
    f.read_exact(&mut buf)?;
    eat(&buf);
    if size > CHUNK {
        let tail = CHUNK.min(size - CHUNK);
        f.seek(SeekFrom::End(-(tail as i64)))?;
        let mut buf = vec![0u8; tail as usize];
        f.read_exact(&mut buf)?;
        eat(&buf);
    }
    Ok(format!("{hash:016x}{size:012x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_depends_on_content_and_size() {
        let dir = std::env::temp_dir().join(format!("tsuki-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.ts");
        let b = dir.join("b.ts");
        fs::write(&a, vec![1u8; 3 << 20]).unwrap();
        fs::write(&b, vec![1u8; 3 << 20]).unwrap();
        assert_eq!(fingerprint(&a).unwrap(), fingerprint(&b).unwrap()); // même contenu
        fs::write(&b, vec![2u8; 3 << 20]).unwrap();
        assert_ne!(fingerprint(&a).unwrap(), fingerprint(&b).unwrap());
        assert_eq!(safe_id("ab/../cd"), "abcd");
        fs::remove_dir_all(dir).unwrap();
    }
}
