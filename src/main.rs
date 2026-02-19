mod config;
mod db;
mod scanner;
mod ui;

use config::{load_config, save_config, Config};
use db::init_db;
use eframe::egui;
use notify::{RecursiveMode, Watcher};
use rusqlite::Connection;
use scanner::{scan_directory, start_watcher};
use std::sync::{Arc, Mutex};
use ui::SongIndexApp;

fn main() {
    let base_dir = match load_config() {
        Some(cfg) if cfg.music_dir.is_dir() => cfg.music_dir,
        _ => {
            eprintln!("Songindex: no config found, opening folder picker...");
            match rfd::FileDialog::new()
                .set_title("Musikordner auswählen")
                .pick_folder()
            {
                Some(dir) => {
                    save_config(&Config {
                        music_dir: dir.clone(),
                    });
                    dir
                }
                None => {
                    eprintln!("Songindex: no folder selected, exiting.");
                    return;
                }
            }
        }
    };

    eprintln!("Songindex: scanning {}", base_dir.display());

    let db_path = std::env::current_dir().unwrap().join("songindex.db");
    let conn = Connection::open(&db_path).expect("Failed to open database");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .ok();
    init_db(&conn);

    eprintln!("Songindex: initial scan...");
    scan_directory(&conn, &base_dir);

    let song_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM songs", [], |row| row.get(0))
        .unwrap_or(0);
    eprintln!("Songindex: {} songs indexed", song_count);

    let db = Arc::new(Mutex::new(conn));

    let (notify_tx, notify_rx) = std::sync::mpsc::channel();

    let mut watcher = start_watcher(db.clone(), base_dir.clone(), notify_tx);
    watcher
        .watch(&base_dir, RecursiveMode::Recursive)
        .expect("Failed to watch directory");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Songindex")
            .with_inner_size([900.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Songindex",
        options,
        Box::new(move |_cc| {
            // Keep watcher alive by moving it into the closure
            let _watcher = watcher;
            Ok(Box::new(SongIndexApp::new(db, base_dir, notify_rx)))
        }),
    )
    .expect("Failed to run eframe");
}
