use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    pub id: i64,
    pub titel: String,
    pub artist: Option<String>,
    pub dateipfad: String,
    pub dateiname: String,
    pub has_audio: bool,
    pub audio_pfad: Option<String>,
    pub tags: Vec<TagInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagInfo {
    pub id: i64,
    pub kategorie: String,
    pub wert: String,
    pub auto_generated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagGroup {
    pub kategorie: String,
    pub tags: Vec<TagEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagEntry {
    pub id: i64,
    pub wert: String,
    pub count: i64,
}

#[derive(Debug, Clone)]
pub struct Stats {
    pub total_songs: i64,
    pub songs_with_audio: i64,
    pub untagged_songs: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortMode {
    Title,
    Artist,
    Recent,
    Untagged,
}

impl SortMode {
    pub fn label(&self) -> &str {
        match self {
            SortMode::Title => "Titel",
            SortMode::Artist => "Artist",
            SortMode::Recent => "Neueste zuerst",
            SortMode::Untagged => "Ohne Tags zuerst",
        }
    }

    pub fn all() -> &'static [SortMode] {
        &[SortMode::Title, SortMode::Artist, SortMode::Recent, SortMode::Untagged]
    }
}

pub fn init_db(conn: &Connection) {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS songs (
            id INTEGER PRIMARY KEY,
            titel TEXT NOT NULL,
            artist TEXT,
            dateipfad TEXT NOT NULL UNIQUE,
            dateiname TEXT NOT NULL,
            has_audio INTEGER DEFAULT 0,
            audio_pfad TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY,
            kategorie TEXT NOT NULL,
            wert TEXT NOT NULL,
            UNIQUE(kategorie, wert)
        );

        CREATE TABLE IF NOT EXISTS song_tags (
            song_id INTEGER REFERENCES songs(id) ON DELETE CASCADE,
            tag_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
            auto_generated INTEGER DEFAULT 0,
            PRIMARY KEY (song_id, tag_id)
        );

        CREATE INDEX IF NOT EXISTS idx_songs_dateipfad ON songs(dateipfad);
        CREATE INDEX IF NOT EXISTS idx_songs_titel ON songs(titel);
        CREATE INDEX IF NOT EXISTS idx_tags_kategorie ON tags(kategorie);
        ",
    )
    .expect("Failed to initialize database");
}

pub fn get_or_create_tag(conn: &Connection, kategorie: &str, wert: &str) -> i64 {
    conn.execute(
        "INSERT OR IGNORE INTO tags (kategorie, wert) VALUES (?1, ?2)",
        params![kategorie, wert],
    )
    .ok();
    conn.query_row(
        "SELECT id FROM tags WHERE kategorie = ?1 AND wert = ?2",
        params![kategorie, wert],
        |row| row.get(0),
    )
    .unwrap()
}

pub fn get_song_tags(conn: &Connection, song_id: i64) -> Vec<TagInfo> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.kategorie, t.wert, st.auto_generated
             FROM tags t
             JOIN song_tags st ON t.id = st.tag_id
             WHERE st.song_id = ?1
             ORDER BY t.kategorie, t.wert",
        )
        .unwrap();

    stmt.query_map(params![song_id], |row| {
        Ok(TagInfo {
            id: row.get(0)?,
            kategorie: row.get(1)?,
            wert: row.get(2)?,
            auto_generated: row.get::<_, i64>(3)? != 0,
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

pub fn query_songs(
    conn: &Connection,
    search: &str,
    tag_ids: &[i64],
    has_audio: bool,
    untagged: bool,
    sort: &SortMode,
) -> Vec<Song> {
    let mut sql = String::from(
        "SELECT DISTINCT s.id, s.titel, s.artist, s.dateipfad, s.dateiname, s.has_audio, s.audio_pfad
         FROM songs s
         LEFT JOIN song_tags st ON s.id = st.song_id
         LEFT JOIN tags t ON st.tag_id = t.id
         WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if !search.is_empty() {
        let n = param_values.len() + 1;
        sql.push_str(&format!(
            " AND (LOWER(s.titel) LIKE ?{n} OR LOWER(s.artist) LIKE ?{n} OR LOWER(s.dateiname) LIKE ?{n})"
        ));
        param_values.push(Box::new(format!("%{}%", search.to_lowercase())));
    }

    if !tag_ids.is_empty() {
        let placeholders: Vec<String> = tag_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", param_values.len() + i + 1))
            .collect();
        let ph = placeholders.join(",");

        sql.push_str(&format!(
            " AND s.id IN (
                SELECT song_id FROM song_tags WHERE tag_id IN ({ph})
                GROUP BY song_id
                HAVING COUNT(DISTINCT (SELECT kategorie FROM tags WHERE id = tag_id)) = (
                    SELECT COUNT(DISTINCT kategorie) FROM tags WHERE id IN ({ph})
                )
            )"
        ));

        for &id in tag_ids {
            param_values.push(Box::new(id));
        }
    }

    if has_audio {
        sql.push_str(" AND s.has_audio = 1");
    }

    if untagged {
        sql.push_str(
            " AND s.id NOT IN (SELECT DISTINCT song_id FROM song_tags WHERE auto_generated = 0)",
        );
    }

    let order = match sort {
        SortMode::Artist => "ORDER BY COALESCE(s.artist, 'zzz'), s.titel",
        SortMode::Recent => "ORDER BY s.created_at DESC",
        SortMode::Untagged => {
            "ORDER BY (SELECT COUNT(*) FROM song_tags WHERE song_id = s.id) ASC, s.titel"
        }
        SortMode::Title => "ORDER BY s.titel",
    };
    sql.push_str(&format!(" {order}"));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&sql).unwrap();
    let song_rows: Vec<(i64, String, Option<String>, String, String, bool, Option<String>)> = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get::<_, i64>(5)? != 0,
                row.get(6)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    let mut songs = Vec::new();
    for (id, titel, artist, dateipfad, dateiname, has_audio_val, audio_pfad) in song_rows {
        let tags = get_song_tags(conn, id);
        songs.push(Song {
            id,
            titel,
            artist,
            dateipfad,
            dateiname,
            has_audio: has_audio_val,
            audio_pfad,
            tags,
        });
    }

    songs
}

pub fn update_song(conn: &Connection, id: i64, titel: &str, artist: &str) {
    conn.execute(
        "UPDATE songs SET titel = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        params![titel, id],
    )
    .ok();

    let artist_val: Option<&str> = if artist.is_empty() { None } else { Some(artist) };
    conn.execute(
        "UPDATE songs SET artist = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        params![artist_val, id],
    )
    .ok();
}

pub fn add_tag_to_song(conn: &Connection, song_id: i64, kategorie: &str, wert: &str) {
    let tag_id = get_or_create_tag(conn, kategorie, wert);
    conn.execute(
        "INSERT OR IGNORE INTO song_tags (song_id, tag_id, auto_generated) VALUES (?1, ?2, 0)",
        params![song_id, tag_id],
    )
    .ok();
}

pub fn remove_tag_from_song(conn: &Connection, song_id: i64, tag_id: i64) {
    conn.execute(
        "DELETE FROM song_tags WHERE song_id = ?1 AND tag_id = ?2",
        params![song_id, tag_id],
    )
    .ok();

    conn.execute(
        "DELETE FROM tags WHERE id = ?1 AND id NOT IN (SELECT DISTINCT tag_id FROM song_tags)",
        params![tag_id],
    )
    .ok();
}

pub fn get_all_tags(conn: &Connection) -> Vec<TagGroup> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.kategorie, t.wert, COUNT(st.song_id) as cnt
             FROM tags t
             LEFT JOIN song_tags st ON t.id = st.tag_id
             GROUP BY t.id
             ORDER BY t.kategorie, cnt DESC, t.wert",
        )
        .unwrap();

    let rows: Vec<(i64, String, String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    let mut groups: HashMap<String, Vec<TagEntry>> = HashMap::new();
    for (id, kategorie, wert, count) in rows {
        groups
            .entry(kategorie)
            .or_default()
            .push(TagEntry { id, wert, count });
    }

    let category_order = [
        "instrument",
        "schwierigkeit",
        "stil",
        "technik",
        "artist",
        "stimmung",
        "kapo",
    ];

    let mut result: Vec<TagGroup> = Vec::new();
    for cat in &category_order {
        if let Some(tags) = groups.remove(*cat) {
            result.push(TagGroup {
                kategorie: cat.to_string(),
                tags,
            });
        }
    }
    for (kategorie, tags) in groups {
        result.push(TagGroup { kategorie, tags });
    }

    result
}

pub fn get_stats(conn: &Connection) -> Stats {
    let total_songs: i64 = conn
        .query_row("SELECT COUNT(*) FROM songs", [], |row| row.get(0))
        .unwrap_or(0);

    let untagged_songs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM songs WHERE id NOT IN (SELECT DISTINCT song_id FROM song_tags)",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let songs_with_audio: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM songs WHERE has_audio = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Stats {
        total_songs,
        songs_with_audio,
        untagged_songs,
    }
}
