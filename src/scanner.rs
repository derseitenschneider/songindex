use crate::db::get_or_create_tag;
use notify::{Event, EventKind, RecommendedWatcher};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

/// Normalize a lossy string from the filesystem to NFC form.
/// macOS stores filenames in NFD (decomposed), which causes combining
/// characters (e.g. "a" + U+0308) to render as boxes in egui.
fn nfc(s: std::borrow::Cow<'_, str>) -> String {
    s.nfc().collect()
}

struct AutoTag {
    pattern: &'static str,
    kategorie: &'static str,
    wert: &'static str,
}

const AUTO_TAGS: &[AutoTag] = &[
    AutoTag { pattern: "E-Gitarre", kategorie: "instrument", wert: "E-Gitarre" },
    AutoTag { pattern: "e-gitarre", kategorie: "instrument", wert: "E-Gitarre" },
    AutoTag { pattern: "1. E-Gitarre", kategorie: "instrument", wert: "E-Gitarre" },
    AutoTag { pattern: "Zupfen", kategorie: "technik", wert: "Fingerpicking" },
    AutoTag { pattern: "zupfen", kategorie: "technik", wert: "Fingerpicking" },
    AutoTag { pattern: "Anfaenger", kategorie: "schwierigkeit", wert: "Anfänger" },
    AutoTag { pattern: "Kinderlieder", kategorie: "schwierigkeit", wert: "Anfänger" },
    AutoTag { pattern: "Kinderlieder", kategorie: "stil", wert: "Kinderlieder" },
    AutoTag { pattern: "Moderne Popsongs", kategorie: "stil", wert: "Pop" },
    AutoTag { pattern: "Mundart", kategorie: "stil", wert: "Mundart" },
    AutoTag { pattern: "Weihnachtssongs", kategorie: "stil", wert: "Weihnachten" },
    AutoTag { pattern: "Christmas", kategorie: "stil", wert: "Weihnachten" },
    AutoTag { pattern: "Worship", kategorie: "stil", wert: "Worship" },
    AutoTag { pattern: "Blues", kategorie: "stil", wert: "Blues" },
    AutoTag { pattern: "Jazz", kategorie: "stil", wert: "Jazz" },
    AutoTag { pattern: "Solos", kategorie: "technik", wert: "Solo" },
    AutoTag { pattern: "7. Solos", kategorie: "technik", wert: "Solo" },
    AutoTag { pattern: "Klassisch", kategorie: "stil", wert: "Klassik" },
    AutoTag { pattern: "ukulele", kategorie: "instrument", wert: "Ukulele" },
    AutoTag { pattern: "01 ukulele", kategorie: "instrument", wert: "Ukulele" },
    AutoTag { pattern: "The Beatles", kategorie: "artist", wert: "The Beatles" },
    AutoTag { pattern: "Bossa", kategorie: "stil", wert: "Bossa Nova" },
    AutoTag { pattern: "Samba", kategorie: "stil", wert: "Bossa Nova" },
];

pub fn parse_filename(filename: &str) -> (String, Option<String>) {
    let name = filename
        .trim_end_matches(".pdf")
        .trim_end_matches(".PDF")
        .trim_end_matches(" Kopie")
        .trim();

    if let Some(idx) = name.find(" - ") {
        let artist = name[..idx].trim().to_string();
        let titel = name[idx + 3..].trim().to_string();
        if !artist.is_empty() && !titel.is_empty() {
            return (titel, Some(artist));
        }
    }
    if let Some(idx) = name.find(" – ") {
        let artist = name[..idx].trim().to_string();
        let titel = name[idx + "\u{2013}".len() + 2..].trim().to_string();
        if !artist.is_empty() && !titel.is_empty() {
            return (titel, Some(artist));
        }
    }

    (name.to_string(), None)
}

fn infer_tags(path: &str) -> Vec<(&str, &str)> {
    let mut tags = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for auto_tag in AUTO_TAGS {
        if path.contains(auto_tag.pattern) {
            let key = (auto_tag.kategorie, auto_tag.wert);
            if seen.insert(key) {
                tags.push(key);
            }
        }
    }

    if path.contains("00 gitarre") && !tags.iter().any(|(k, _)| *k == "instrument") {
        tags.push(("instrument", "Akustik-Gitarre"));
    }

    tags
}

fn find_audio_match(base_dir: &Path, song_title: &str) -> Option<String> {
    let audio_dir = base_dir.join("00 gitarre/0. Songs/2. Audios");
    if !audio_dir.exists() {
        return None;
    }

    let title_lower = song_title.to_lowercase();

    for entry in WalkDir::new(&audio_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            if ext_lower == "mp3" || ext_lower == "wav" || ext_lower == "m4a" {
                if let Some(stem) = path.file_stem() {
                    if nfc(stem.to_string_lossy()).to_lowercase().contains(&title_lower) {
                        if let Ok(rel) = path.strip_prefix(base_dir) {
                            return Some(nfc(rel.to_string_lossy()));
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn scan_directory(conn: &Connection, base_dir: &Path) {
    let mut found_paths: Vec<String> = Vec::new();

    for entry in WalkDir::new(base_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if path.starts_with(base_dir.join("songindex")) {
            continue;
        }
        if path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            continue;
        }

        if let Some(ext) = path.extension() {
            if ext.to_string_lossy().to_lowercase() != "pdf" {
                continue;
            }
        } else {
            continue;
        }

        let rel_path = match path.strip_prefix(base_dir) {
            Ok(r) => nfc(r.to_string_lossy()),
            Err(_) => continue,
        };

        let filename = nfc(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy(),
        );

        found_paths.push(rel_path.clone());

        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM songs WHERE dateipfad = ?1",
                params![&rel_path],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;

        if exists {
            continue;
        }

        let (titel, artist) = parse_filename(&filename);
        let audio_match = find_audio_match(base_dir, &titel);
        let has_audio = audio_match.is_some();

        conn.execute(
            "INSERT INTO songs (titel, artist, dateipfad, dateiname, has_audio, audio_pfad) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![titel, artist, rel_path, filename, has_audio, audio_match],
        ).ok();

        let song_id: i64 = conn.last_insert_rowid();

        let tags = infer_tags(&rel_path);
        for (kategorie, wert) in tags {
            let tag_id = get_or_create_tag(conn, kategorie, wert);
            conn.execute(
                "INSERT OR IGNORE INTO song_tags (song_id, tag_id, auto_generated) VALUES (?1, ?2, 1)",
                params![song_id, tag_id],
            ).ok();
        }
    }

    let mut stmt = conn
        .prepare("SELECT id, dateipfad FROM songs")
        .unwrap();
    let db_songs: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for (id, path) in db_songs {
        if !found_paths.contains(&path) {
            conn.execute("DELETE FROM songs WHERE id = ?1", params![id])
                .ok();
        }
    }

    conn.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM song_tags)",
        [],
    )
    .ok();
}

fn add_single_file(conn: &Connection, base_dir: &Path, file_path: &Path) {
    if let Some(ext) = file_path.extension() {
        if ext.to_string_lossy().to_lowercase() != "pdf" {
            return;
        }
    } else {
        return;
    }

    if file_path.starts_with(base_dir.join("songindex")) {
        return;
    }

    let rel_path = match file_path.strip_prefix(base_dir) {
        Ok(r) => nfc(r.to_string_lossy()),
        Err(_) => return,
    };

    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM songs WHERE dateipfad = ?1",
            params![&rel_path],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    if exists {
        return;
    }

    let filename = nfc(
        file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
    );

    let (titel, artist) = parse_filename(&filename);
    let audio_match = find_audio_match(base_dir, &titel);
    let has_audio = audio_match.is_some();

    conn.execute(
        "INSERT INTO songs (titel, artist, dateipfad, dateiname, has_audio, audio_pfad) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![titel, artist, rel_path, filename, has_audio, audio_match],
    ).ok();

    let song_id: i64 = conn.last_insert_rowid();

    let tags = infer_tags(&rel_path);
    for (kategorie, wert) in tags {
        let tag_id = get_or_create_tag(conn, kategorie, wert);
        conn.execute(
            "INSERT OR IGNORE INTO song_tags (song_id, tag_id, auto_generated) VALUES (?1, ?2, 1)",
            params![song_id, tag_id],
        )
        .ok();
    }

    eprintln!("Added: {}", rel_path);
}

fn remove_single_file(conn: &Connection, base_dir: &Path, file_path: &Path) {
    let rel_path = match file_path.strip_prefix(base_dir) {
        Ok(r) => nfc(r.to_string_lossy()),
        Err(_) => return,
    };

    conn.execute("DELETE FROM songs WHERE dateipfad = ?1", params![rel_path])
        .ok();
    eprintln!("Removed: {}", rel_path);
}

/// Start the file watcher on a background thread. Returns the watcher handle (must be kept alive).
/// Sends a signal via `notify_tx` whenever files change so the UI can refresh.
pub fn start_watcher(
    db: Arc<Mutex<Connection>>,
    base_dir: std::path::PathBuf,
    notify_tx: std::sync::mpsc::Sender<()>,
) -> RecommendedWatcher {
    let (tx, rx) = std::sync::mpsc::channel::<std::path::PathBuf>();

    let db_thread = db.clone();
    let base_dir_thread = base_dir.clone();
    std::thread::spawn(move || {
        while let Ok(path) = rx.recv() {
            let conn = db_thread.lock().unwrap();
            if path.exists() {
                add_single_file(&conn, &base_dir_thread, &path);
            } else {
                remove_single_file(&conn, &base_dir_thread, &path);
            }
            drop(conn);
            let _ = notify_tx.send(());
        }
    });

    let base_dir_notify = base_dir.clone();
    let watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        for path in event.paths {
                            if path.starts_with(base_dir_notify.join("songindex")) {
                                continue;
                            }
                            if let Some(ext) = path.extension() {
                                if ext.to_string_lossy().to_lowercase() == "pdf" {
                                    let _ = tx.send(path);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        })
        .expect("Failed to create file watcher");

    watcher
}
