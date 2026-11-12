//! Niveau 3 de vérification : ce chat correspond-il bien à cette vidéo ?
//!
//! Contrôle principal : l'heure réelle. Twitch inscrit dans chaque segment vidéo son
//! heure de diffusion (étiquette ID3). Le chat permet de la recalculer :
//! début du stream (`video.uploadDate`) + début du passage (`startTime`).
//! Les deux doivent coïncider à quelques secondes près.
//!
//! Contrôle de secours, si la vidéo n'a pas d'étiquette ID3 : le numéro de VOD au
//! début du nom du fichier vidéo (Xtra nomme les fichiers `<id><qualité><date>.ts`).

use serde::Serialize;

use crate::chat::Chat;
use crate::ts_index::TsIndex;

/// Écart toléré entre l'heure de la vidéo et celle calculée depuis le chat.
pub const TOLERANCE_SECS: i64 = 60;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Verdict {
    /// Correspondance confirmée.
    Match { method: Method, drift_secs: Option<i64> },
    /// Le chat appartient à une autre vidéo.
    Mismatch { reason: String },
    /// Impossible de vérifier : refusé par prudence.
    Unverifiable { reason: String },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Wallclock,
    FileName,
}

impl Verdict {
    pub fn is_ok(&self) -> bool {
        matches!(self, Verdict::Match { .. })
    }
}

pub fn check(chat: &Chat, video: &TsIndex, video_file_name: &str) -> Verdict {
    let expected = chat
        .video
        .upload_date
        .map(|ms| ms.div_euclid(1000) + chat.start_time.round() as i64);
    let actual = video.id3.wallclock.as_deref().and_then(parse_utc);

    if let (Some(expected), Some(actual)) = (expected, actual) {
        let drift = actual - expected;
        return if drift.abs() <= TOLERANCE_SECS {
            Verdict::Match { method: Method::Wallclock, drift_secs: Some(drift) }
        } else {
            Verdict::Mismatch {
                reason: format!(
                    "la vidéo a été diffusée le {}, alors que ce chat correspond au {}",
                    format_utc(actual),
                    format_utc(expected)
                ),
            }
        };
    }

    // Secours : le nom du fichier vidéo commence-t-il par le numéro de la VOD ?
    let id = chat.video.id.trim();
    let digits: String = video_file_name.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !id.is_empty() && digits.len() > id.len() {
        return if digits.starts_with(id) {
            Verdict::Match { method: Method::FileName, drift_secs: None }
        } else {
            Verdict::Mismatch {
                reason: format!("ce chat est celui de la VOD n° {id}, mais la vidéo ne porte pas ce numéro"),
            }
        };
    }

    Verdict::Unverifiable {
        reason: "la vidéo ne contient pas d'heure de diffusion et son nom ne permet pas de \
                 l'identifier"
            .into(),
    }
}

/// `2026-11-06T21:18:20` (UTC, fraction et `Z` facultatifs) → secondes Unix.
pub fn parse_utc(s: &str) -> Option<i64> {
    let s = s.trim().trim_end_matches('Z');
    let (date, time) = s.split_once(['T', ' '])?;
    let mut d = date.splitn(3, '-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let time = time.split(['.', '+']).next()?;
    let mut t = time.splitn(3, ':').map(|x| x.parse::<i64>().ok());
    let (h, mi, sec) = (t.next()??, t.next()??, t.next().flatten().unwrap_or(0));
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    Some(days_from_civil(y, m, day) * 86_400 + h * 3600 + mi * 60 + sec)
}

/// Affichage lisible d'une date UTC : `06/11/2026 à 21:18 UTC`.
pub fn format_utc(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    format!("{d:02}/{m:02}/{y} à {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}

// Algorithmes de Howard Hinnant (calendrier grégorien proleptique).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_roundtrip() {
        // uploadDate du chat d'exemple : 1793995223000 ms = 2026-11-06 20:00:23 UTC.
        assert_eq!(parse_utc("2026-11-06T20:00:23"), Some(1_793_995_223));
        assert_eq!(format_utc(1_793_995_223), "06/11/2026 à 20:00 UTC");
        assert_eq!(parse_utc("2026-11-06T20:00:38.479Z"), Some(parse_utc("2026-11-06T20:00:38").unwrap()));
        assert_eq!(parse_utc("1970-01-01T00:00:00"), Some(0));
        assert_eq!(parse_utc("pas une date"), None);
    }
}
