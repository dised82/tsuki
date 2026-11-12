//! Outil de diagnostic : `cargo run --example inspect -- video.ts [chat.json] [--m3u8 sortie.m3u8]`

use std::path::Path;
use std::time::Instant;

use tsuki_core::{chat, ts_index, validate};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let m3u8_out = args.iter().position(|a| a == "--m3u8").and_then(|i| args.get(i + 1)).cloned();
    let files: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && args.get(i.wrapping_sub(1)).is_none_or(|p| p != "--m3u8"))
        .map(|(_, a)| a)
        .collect();
    let Some(video_path) = files.first().map(Path::new) else {
        eprintln!("usage : inspect video.ts [chat.json] [--m3u8 sortie.m3u8]");
        std::process::exit(2);
    };

    let t = Instant::now();
    let mode = if args.iter().any(|a| a == "--sparse") { "extraits" } else if args.iter().any(|a| a == "--full") { "complète" } else { "auto" };
    let result = match mode {
        "extraits" => ts_index::index_sparse(video_path),
        "complète" => ts_index::index_file_full(video_path),
        _ => ts_index::index_file(video_path),
    };
    let index = match result {
        Ok(i) => i,
        Err(e) => {
            eprintln!("vidéo : {e}");
            std::process::exit(1);
        }
    };
    println!("== Vidéo (analyse {mode} : {:?})", t.elapsed());
    println!("  taille       : {:.1} Mo", index.file_size as f64 / 1e6);
    println!("  durée        : {:.3} s", index.duration);
    println!("  1er PTS brut : {:.3} s", index.first_pts);
    println!("  images clés  : {}  |  audio : {}", index.keyframes, index.has_audio);
    println!("  segments     : {}", index.segments.len());
    for s in index.segments.iter().take(3).chain(index.segments.last()) {
        println!("    @{:>10} +{:>9} o  t={:>8.3}  d={:.3}", s.offset, s.len, s.start, s.duration);
    }
    println!("  ID3          : {:?}", index.id3);
    let covered: u64 = index.segments.iter().map(|s| s.len).sum();
    println!("  couverture   : {covered} / {} octets", index.file_size);

    if let Some(out) = &m3u8_out {
        let abs = std::fs::canonicalize(video_path).unwrap();
        std::fs::write(out, index.to_m3u8(&format!("file://{}", abs.display()))).unwrap();
        println!("  playlist écrite dans {out}");
    }

    let Some(chat_path) = files.get(1).map(Path::new) else { return };
    let t = Instant::now();
    let chat = match chat::read_file(chat_path) {
        Ok(c) => c,
        Err(e) => {
            println!("== Chat : REFUSÉ — {e}");
            std::process::exit(1);
        }
    };
    println!("== Chat ({:?} de lecture)", t.elapsed());
    println!("  vidéo        : {} — {:?} ({:?})", chat.video.id, chat.video.title, chat.video.channel_name);
    println!("  startTime    : {} s", chat.start_time);
    println!("  messages     : {}", chat.comments.len());
    if let (Some(a), Some(b)) = (chat.comments.first(), chat.comments.last()) {
        println!("  plage        : {} s → {} s", a.offset, b.offset);
    }
    let lo = chat.start_time;
    let hi = lo + index.duration;
    println!("  pendant la vidéo [{lo}, {hi:.1}] : {} messages", chat.upto(hi) - chat.upto(lo - 1e-9));
    println!("  emotes       : {}  |  badges : {}", chat.emotes.len(), chat.badges.len());
    if !chat.unknown_keys.is_empty() {
        println!("  clés inconnues : {:?}", chat.unknown_keys);
    }
    let name = video_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    println!("== Correspondance : {:?}", validate::check(&chat, &index, name));
}
