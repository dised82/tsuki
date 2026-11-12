//! Analyse d'un fichier MPEG-TS (format des VODs Twitch téléchargées par Xtra).
//!
//! On parcourt le fichier une seule fois pour construire une **table de navigation** :
//! la position (en octets) et l'horodatage de chaque image clé. Cette table permet
//! ensuite de servir la vidéo à hls.js sous forme de playlist HLS « byte-range »,
//! sans conversion ni copie du fichier.
//!
//! On récupère aussi les étiquettes ID3 que Twitch insère dans chaque segment :
//! l'heure réelle du segment et son numéro, utilisés pour vérifier le chat.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use serde::Serialize;

const PACKET: usize = 188;
const SYNC: u8 = 0x47;
/// Les horodatages MPEG-TS sont en 90 kHz.
const CLOCK: f64 = 90_000.0;
/// Durée visée des segments de la playlist (on regroupe les images clés).
const TARGET_SEGMENT_SECS: f64 = 6.0;
/// Quantité de données vidéo examinée pour détecter une image clé.
const NAL_SCAN_BYTES: usize = 16 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum TsError {
    #[error("lecture du fichier impossible : {0}")]
    Io(#[from] io::Error),
    #[error("ce fichier n'est pas un flux MPEG-TS")]
    NotTs,
    #[error("aucune piste vidéo trouvée dans le fichier")]
    NoVideo,
    #[error("aucune image clé trouvée : la vidéo est illisible ou vide")]
    NoKeyframe,
}

/// Un morceau du fichier servi à hls.js.
#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub offset: u64,
    pub len: u64,
    /// Début, en secondes, depuis le début de la vidéo.
    pub start: f64,
    pub duration: f64,
}

/// Informations lues dans les étiquettes ID3 du premier segment.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Id3Info {
    /// Heure réelle (UTC) du premier segment, ex. `2026-11-06T21:18:20`.
    pub wallclock: Option<String>,
    /// Numéro du segment Twitch (`index-0000002340.ts` → 2340).
    pub segment_number: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TsIndex {
    pub file_size: u64,
    /// Durée de la vidéo en secondes.
    pub duration: f64,
    /// Horodatage brut de la première image (secondes), avant remise à zéro.
    pub first_pts: f64,
    pub keyframes: usize,
    pub has_audio: bool,
    pub segments: Vec<Segment>,
    pub id3: Id3Info,
}

impl TsIndex {
    /// Playlist HLS (VOD) qui découpe le fichier en morceaux sans le copier.
    /// `uri` est l'adresse à laquelle le fichier complet est servi (requêtes « Range »).
    pub fn to_m3u8(&self, uri: &str) -> String {
        self.playlist(|_, s| format!("#EXT-X-BYTERANGE:{}@{}\n{uri}", s.len, s.offset))
    }

    /// Variante où chaque morceau a sa propre adresse (`uri_for(numéro)`) :
    /// le serveur lit lui-même la bonne plage d'octets.
    pub fn to_m3u8_split(&self, uri_for: impl Fn(usize) -> String) -> String {
        self.playlist(|i, _| uri_for(i))
    }

    fn playlist(&self, entry: impl Fn(usize, &Segment) -> String) -> String {
        let target = self
            .segments
            .iter()
            .map(|s| s.duration)
            .fold(1.0_f64, f64::max)
            .ceil() as u64;
        let mut out = String::with_capacity(64 + self.segments.len() * 80);
        out.push_str("#EXTM3U\n#EXT-X-VERSION:4\n#EXT-X-PLAYLIST-TYPE:VOD\n");
        out.push_str("#EXT-X-INDEPENDENT-SEGMENTS\n");
        out.push_str(&format!("#EXT-X-TARGETDURATION:{target}\n#EXT-X-MEDIA-SEQUENCE:0\n"));
        for (i, s) in self.segments.iter().enumerate() {
            out.push_str(&format!("#EXTINF:{:.6},\n{}\n", s.duration, entry(i, s)));
        }
        out.push_str("#EXT-X-ENDLIST\n");
        out
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Video,
    Audio,
    Id3,
}

struct Keyframe {
    boundary: u64,
    pts: u64,
}

/// État d'un paquet PES vidéo en cours de lecture.
struct VideoPes {
    boundary: u64,
    pts: Option<u64>,
    random_access: bool,
    head: Vec<u8>,
}

#[derive(Default)]
struct Parser {
    pmt_pid: Option<u16>,
    streams: Vec<(u16, Kind)>,
    last_pat_offset: Option<u64>,
    current: Option<VideoPes>,
    keyframes: Vec<Keyframe>,
    /// Horodatages vidéo déroulés (sans retour à zéro après 2^33).
    first_pts: Option<u64>,
    max_pts: u64,
    last_raw_pts: Option<u64>,
    wrap: u64,
    frames: u64,
    id3_buf: Vec<u8>,
    id3_state: u8, // 0 = pas commencé, 1 = en cours, 2 = terminé
}

pub fn index_file(path: &Path) -> Result<TsIndex, TsError> {
    let file = File::open(path)?;
    let file_size = file.metadata()?.len();
    index_reader(BufReader::with_capacity(1 << 20, file), file_size)
}

pub fn index_reader<R: Read>(mut reader: R, file_size: u64) -> Result<TsIndex, TsError> {
    let mut p = Parser::default();
    let mut pkt = [0u8; PACKET];
    let mut offset: u64 = 0;
    let mut seen_sync = 0usize;

    loop {
        match read_full(&mut reader, &mut pkt)? {
            0 => break,
            n if n < PACKET => break, // paquet tronqué en fin de fichier (téléchargement interrompu)
            _ => {}
        }
        if pkt[0] != SYNC {
            // Resynchronisation : on avance octet par octet jusqu'au prochain 0x47.
            if seen_sync == 0 && offset > (PACKET * 16) as u64 {
                return Err(TsError::NotTs);
            }
            let skip = resync(&mut reader, &mut pkt)?;
            offset += skip;
            if skip == 0 {
                break;
            }
        }
        seen_sync += 1;
        p.packet(&pkt, offset);
        offset += PACKET as u64;
    }

    if seen_sync == 0 {
        return Err(TsError::NotTs);
    }
    p.finish_pes();
    if !p.streams.iter().any(|(_, k)| *k == Kind::Video) {
        return Err(TsError::NoVideo);
    }
    if p.keyframes.is_empty() {
        return Err(TsError::NoKeyframe);
    }
    let first_pts = p.first_pts.unwrap_or(p.keyframes[0].pts);
    // Fin de la vidéo = dernière image + durée moyenne d'une image.
    let span = p.max_pts.saturating_sub(first_pts);
    let frame = if p.frames > 1 { span / (p.frames - 1) } else { 0 };
    let end = p.max_pts + frame;

    let segments = build_segments(&p.keyframes, first_pts, end, file_size.max(offset));
    let id3 = parse_id3(&p.id3_buf);
    Ok(TsIndex {
        file_size: file_size.max(offset),
        duration: (end - first_pts) as f64 / CLOCK,
        first_pts: first_pts as f64 / CLOCK,
        keyframes: p.keyframes.len(),
        has_audio: p.streams.iter().any(|(_, k)| *k == Kind::Audio),
        segments,
        id3,
    })
}

fn read_full<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// Décale `pkt` jusqu'à ce qu'il commence par l'octet de synchronisation.
/// Renvoie le nombre d'octets sautés (0 = fin de fichier).
fn resync<R: Read>(r: &mut R, pkt: &mut [u8; PACKET]) -> io::Result<u64> {
    let mut skipped = 0u64;
    loop {
        if let Some(pos) = pkt.iter().position(|&b| b == SYNC) {
            pkt.copy_within(pos.., 0);
            let have = PACKET - pos;
            let got = read_full(r, &mut pkt[have..])?;
            skipped += pos as u64;
            if got < pos {
                return Ok(0);
            }
            return Ok(skipped);
        }
        skipped += PACKET as u64;
        if read_full(r, pkt)? < PACKET {
            return Ok(0);
        }
    }
}

impl Parser {
    fn kind(&self, pid: u16) -> Option<Kind> {
        self.streams.iter().find(|(p, _)| *p == pid).map(|(_, k)| *k)
    }

    fn packet(&mut self, pkt: &[u8; PACKET], offset: u64) {
        let pusi = pkt[1] & 0x40 != 0;
        let pid = (u16::from(pkt[1] & 0x1f) << 8) | u16::from(pkt[2]);
        let afc = (pkt[3] >> 4) & 0x3;
        let mut start = 4usize;
        let mut random_access = false;
        if afc & 0x2 != 0 {
            let af_len = pkt[4] as usize;
            if af_len > 0 {
                random_access = pkt[5] & 0x40 != 0;
            }
            start = 5 + af_len;
        }
        if afc & 0x1 == 0 || start >= PACKET {
            return;
        }
        let payload = &pkt[start..];

        if pid == 0 {
            self.last_pat_offset = Some(offset);
            if pusi {
                self.parse_pat(payload);
            }
            return;
        }
        if Some(pid) == self.pmt_pid {
            if pusi {
                self.parse_pmt(payload);
            }
            return;
        }
        match self.kind(pid) {
            Some(Kind::Video) => self.video(payload, pusi, random_access, offset),
            Some(Kind::Id3) => self.id3(payload, pusi),
            _ => {}
        }
    }

    fn parse_pat(&mut self, payload: &[u8]) {
        let Some(sec) = section(payload) else { return };
        let mut i = 8;
        while i + 4 <= sec.len().saturating_sub(4) {
            let program = u16::from_be_bytes([sec[i], sec[i + 1]]);
            let pid = (u16::from(sec[i + 2] & 0x1f) << 8) | u16::from(sec[i + 3]);
            if program != 0 {
                self.pmt_pid = Some(pid);
                return;
            }
            i += 4;
        }
    }

    fn parse_pmt(&mut self, payload: &[u8]) {
        let Some(sec) = section(payload) else { return };
        if sec.len() < 12 {
            return;
        }
        let info_len = (usize::from(sec[10] & 0x0f) << 8) | usize::from(sec[11]);
        let mut i = 12 + info_len;
        let end = sec.len().saturating_sub(4); // CRC
        let mut streams = Vec::new();
        while i + 5 <= end {
            let stream_type = sec[i];
            let pid = (u16::from(sec[i + 1] & 0x1f) << 8) | u16::from(sec[i + 2]);
            let es_len = (usize::from(sec[i + 3] & 0x0f) << 8) | usize::from(sec[i + 4]);
            let kind = match stream_type {
                0x1b | 0x24 => Some(Kind::Video), // H.264, H.265
                0x0f | 0x11 | 0x03 | 0x04 => Some(Kind::Audio),
                0x15 => Some(Kind::Id3),
                _ => None,
            };
            if let Some(k) = kind {
                streams.push((pid, k));
            }
            i += 5 + es_len;
        }
        if !streams.is_empty() {
            self.streams = streams;
        }
    }

    fn video(&mut self, payload: &[u8], pusi: bool, random_access: bool, offset: u64) {
        if pusi {
            self.finish_pes();
            let (pts, data) = parse_pes_header(payload);
            // Si l'en-tête de flux (PAT/PMT) précède de peu l'image, le morceau
            // commence à l'en-tête : hls.js en a besoin pour décoder le segment.
            let boundary = match self.last_pat_offset {
                Some(pat) if offset - pat <= (PACKET * 64) as u64 => pat,
                _ => offset,
            };
            let mut head = Vec::with_capacity(NAL_SCAN_BYTES);
            head.extend_from_slice(data);
            self.current = Some(VideoPes { boundary, pts, random_access, head });
        } else if let Some(cur) = self.current.as_mut() {
            if cur.head.len() < NAL_SCAN_BYTES {
                cur.head.extend_from_slice(payload);
            }
        }
    }

    fn finish_pes(&mut self) {
        let Some(cur) = self.current.take() else { return };
        let Some(raw) = cur.pts else { return };
        let pts = self.unwrap_pts(raw);
        self.frames += 1;
        self.max_pts = self.max_pts.max(pts);
        if self.first_pts.is_none_or(|f| pts < f) {
            self.first_pts = Some(pts);
        }
        if cur.random_access || has_idr(&cur.head) {
            let boundary = match self.keyframes.last() {
                Some(prev) if cur.boundary <= prev.boundary => return,
                _ => cur.boundary,
            };
            self.keyframes.push(Keyframe { boundary, pts });
        }
    }

    fn unwrap_pts(&mut self, raw: u64) -> u64 {
        const WRAP: u64 = 1 << 33;
        if let Some(last) = self.last_raw_pts {
            if raw + WRAP / 2 < last {
                self.wrap += WRAP;
            }
        }
        self.last_raw_pts = Some(raw);
        raw + self.wrap
    }

    fn id3(&mut self, payload: &[u8], pusi: bool) {
        match (self.id3_state, pusi) {
            (0, true) => {
                let (_, data) = parse_pes_header(payload);
                self.id3_buf.extend_from_slice(data);
                self.id3_state = 1;
            }
            (1, true) => self.id3_state = 2,
            (1, false) if self.id3_buf.len() < 64 * 1024 => self.id3_buf.extend_from_slice(payload),
            _ => {}
        }
    }
}

/// Renvoie la section PSI (à partir du `table_id`), bornée par `section_length`.
fn section(payload: &[u8]) -> Option<&[u8]> {
    let ptr = *payload.first()? as usize;
    let sec = payload.get(1 + ptr..)?;
    if sec.len() < 3 {
        return None;
    }
    let len = (usize::from(sec[1] & 0x0f) << 8) | usize::from(sec[2]);
    sec.get(..(3 + len).min(sec.len()))
}

/// Lit l'en-tête PES : renvoie le PTS éventuel et les données qui suivent.
fn parse_pes_header(p: &[u8]) -> (Option<u64>, &[u8]) {
    if p.len() < 9 || p[0] != 0 || p[1] != 0 || p[2] != 1 {
        return (None, &[]);
    }
    let hdr_len = p[8] as usize;
    let data = p.get(9 + hdr_len..).unwrap_or(&[]);
    if p[7] & 0x80 == 0 || p.len() < 14 {
        return (None, data);
    }
    let pts = (u64::from(p[9] & 0x0e) << 29)
        | (u64::from(p[10]) << 22)
        | (u64::from(p[11] & 0xfe) << 14)
        | (u64::from(p[12]) << 7)
        | (u64::from(p[13]) >> 1);
    (Some(pts), data)
}

/// Une image clé H.264 contient une unité IDR (5) ou commence par un SPS (7).
fn has_idr(data: &[u8]) -> bool {
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            let nal = data[i + 3] & 0x1f;
            if nal == 5 || nal == 7 {
                return true;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    false
}

fn build_segments(kfs: &[Keyframe], first_pts: u64, end_pts: u64, file_size: u64) -> Vec<Segment> {
    // Points de découpe : une image clé toutes les ~TARGET_SEGMENT_SECS secondes.
    let mut cuts: Vec<(u64, u64)> = Vec::new(); // (octet, pts)
    for kf in kfs {
        match cuts.last() {
            None => cuts.push((0, first_pts)), // le premier morceau part du début du fichier
            Some(&(_, pts)) if (kf.pts.saturating_sub(pts)) as f64 / CLOCK >= TARGET_SEGMENT_SECS => {
                cuts.push((kf.boundary, kf.pts))
            }
            _ => {}
        }
    }
    cuts.iter()
        .enumerate()
        .map(|(i, &(off, pts))| {
            let (next_off, next_pts) = cuts.get(i + 1).copied().unwrap_or((file_size, end_pts));
            Segment {
                offset: off,
                len: next_off - off,
                start: (pts - first_pts) as f64 / CLOCK,
                duration: (next_pts.saturating_sub(pts)) as f64 / CLOCK,
            }
        })
        .collect()
}

/// Lecture minimale des balises ID3v2.4 insérées par Twitch.
fn parse_id3(buf: &[u8]) -> Id3Info {
    let mut info = Id3Info::default();
    if buf.len() < 10 || &buf[..3] != b"ID3" {
        return info;
    }
    let size = synchsafe(&buf[6..10]) as usize;
    let end = (10 + size).min(buf.len());
    let mut i = 10;
    while i + 10 <= end {
        let id = &buf[i..i + 4];
        if id[0] == 0 {
            break;
        }
        let len = synchsafe(&buf[i + 4..i + 8]) as usize;
        let body = &buf[(i + 10).min(end)..(i + 10 + len).min(end)];
        let text = || {
            // 1er octet = encodage (0 ou 3 chez Twitch : Latin-1 / UTF-8).
            body.get(1..)
                .map(|t| String::from_utf8_lossy(t).trim_end_matches('\0').trim().to_string())
        };
        match id {
            b"TDTG" => info.wallclock = info.wallclock.take().or_else(text),
            b"TDEN" if info.wallclock.is_none() => info.wallclock = text(),
            b"TOFN" => {
                info.segment_number = text().and_then(|t| {
                    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
                    digits.parse().ok()
                })
            }
            _ => {}
        }
        i += 10 + len;
    }
    info
}

fn synchsafe(b: &[u8]) -> u32 {
    b.iter().fold(0, |acc, &x| (acc << 7) | u32::from(x & 0x7f))
}
