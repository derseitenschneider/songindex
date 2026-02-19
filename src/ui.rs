use crate::db::*;
use eframe::egui;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const CATEGORY_ORDER: &[&str] = &[
    "instrument",
    "schwierigkeit",
    "stil",
    "technik",
];

const CATEGORY_LABELS: &[(&str, &str)] = &[
    ("instrument", "Instrument"),
    ("schwierigkeit", "Schwierigkeit"),
    ("stil", "Stil"),
    ("technik", "Technik"),
    ("stimmung", "Stimmung"),
    ("kapo", "Kapo"),
    ("artist", "Artist"),
];

fn category_label(kategorie: &str) -> &str {
    for (k, l) in CATEGORY_LABELS {
        if *k == kategorie {
            return l;
        }
    }
    kategorie
}

fn tag_color(kategorie: &str) -> egui::Color32 {
    match kategorie {
        "instrument" => egui::Color32::from_rgb(45, 106, 79),    // green
        "schwierigkeit" => egui::Color32::from_rgb(231, 111, 81), // orange
        "stil" => egui::Color32::from_rgb(69, 123, 157),          // blue
        "technik" => egui::Color32::from_rgb(109, 89, 122),       // purple
        "artist" => egui::Color32::from_rgb(188, 108, 37),        // brown
        "stimmung" => egui::Color32::from_rgb(96, 108, 56),       // olive
        "kapo" => egui::Color32::from_rgb(154, 140, 152),         // grey
        _ => egui::Color32::from_rgb(100, 100, 100),
    }
}

struct TagModalState {
    song_id: i64,
    song_titel: String,
    kategorie_idx: usize,
    wert: String,
}

struct EditModalState {
    song_id: i64,
    titel: String,
    artist: String,
}

struct ConfirmRemoveTag {
    song_id: i64,
    tag_id: i64,
    tag_wert: String,
}

pub struct SongIndexApp {
    db: Arc<Mutex<Connection>>,
    base_dir: PathBuf,
    watcher_rx: std::sync::mpsc::Receiver<()>,

    // UI state
    search_text: String,
    active_filters: HashMap<String, HashSet<i64>>,
    filter_audio: bool,
    filter_untagged: bool,
    sort_mode: SortMode,

    // Cached data
    songs: Vec<Song>,
    tags: Vec<TagGroup>,
    stats: Stats,

    // Modals
    tag_modal: Option<TagModalState>,
    edit_modal: Option<EditModalState>,
    confirm_remove: Option<ConfirmRemoveTag>,

    needs_refresh: bool,
}

impl SongIndexApp {
    pub fn new(
        db: Arc<Mutex<Connection>>,
        base_dir: PathBuf,
        watcher_rx: std::sync::mpsc::Receiver<()>,
    ) -> Self {
        let (songs, tags, stats) = {
            let conn = db.lock().unwrap();
            let songs = query_songs(&conn, "", &[], false, false, &SortMode::Title);
            let tags = get_all_tags(&conn);
            let stats = get_stats(&conn);
            (songs, tags, stats)
        };

        Self {
            db,
            base_dir,
            watcher_rx,
            search_text: String::new(),
            active_filters: HashMap::new(),
            filter_audio: false,
            filter_untagged: false,
            sort_mode: SortMode::Title,
            songs,
            tags,
            stats,
            tag_modal: None,
            edit_modal: None,
            confirm_remove: None,
            needs_refresh: false,
        }
    }

    fn refresh_data(&mut self) {
        let tag_ids: Vec<i64> = self
            .active_filters
            .values()
            .flat_map(|s| s.iter().copied())
            .collect();

        let conn = self.db.lock().unwrap();
        self.songs = query_songs(
            &conn,
            &self.search_text,
            &tag_ids,
            self.filter_audio,
            self.filter_untagged,
            &self.sort_mode,
        );
        self.tags = get_all_tags(&conn);
        self.stats = get_stats(&conn);
    }

    fn collect_tag_ids(&self) -> Vec<i64> {
        self.active_filters
            .values()
            .flat_map(|s| s.iter().copied())
            .collect()
    }

    fn refresh_songs_only(&mut self) {
        let tag_ids = self.collect_tag_ids();
        let conn = self.db.lock().unwrap();
        self.songs = query_songs(
            &conn,
            &self.search_text,
            &tag_ids,
            self.filter_audio,
            self.filter_untagged,
            &self.sort_mode,
        );
    }
}

impl eframe::App for SongIndexApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for file watcher notifications
        while self.watcher_rx.try_recv().is_ok() {
            self.needs_refresh = true;
        }
        if self.needs_refresh {
            self.needs_refresh = false;
            self.refresh_data();
        }

        // Dark visuals
        ctx.set_visuals(egui::Visuals::dark());

        // Top panel — header with stats
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Songindex");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} ohne Tags", self.stats.untagged_songs))
                            .small()
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!("{} mit Audio", self.stats.songs_with_audio))
                            .small()
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!("{} Songs", self.stats.total_songs))
                            .small()
                            .color(egui::Color32::from_rgb(233, 69, 96)),
                    );
                });
            });
            ui.add_space(4.0);
        });

        // Central panel
        egui::CentralPanel::default().show(ctx, |ui| {
            // Search bar
            let mut search_changed = false;
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.search_text)
                        .hint_text("Suche nach Titel oder Artist...")
                        .desired_width(ui.available_width() - 80.0),
                );
                if response.changed() {
                    search_changed = true;
                }
                if ui.button("Rescan").clicked() {
                    let conn = self.db.lock().unwrap();
                    crate::scanner::scan_directory(&conn, &self.base_dir);
                    drop(conn);
                    self.refresh_data();
                }
            });

            if search_changed {
                self.refresh_songs_only();
            }

            ui.add_space(6.0);

            // Filter rows
            let mut filter_changed = false;
            for cat_name in CATEGORY_ORDER {
                let group = self.tags.iter().find(|g| g.kategorie == *cat_name);
                if let Some(group) = group {
                    if group.tags.is_empty() {
                        continue;
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}:", category_label(cat_name)))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                        for tag in &group.tags {
                            let is_active = self
                                .active_filters
                                .get(*cat_name)
                                .map_or(false, |s| s.contains(&tag.id));

                            let text = format!("{} ({})", tag.wert, tag.count);
                            let response = ui.selectable_label(is_active, &text);

                            if response.clicked() {
                                let set = self
                                    .active_filters
                                    .entry(cat_name.to_string())
                                    .or_default();
                                if is_active {
                                    set.remove(&tag.id);
                                    if set.is_empty() {
                                        self.active_filters.remove(*cat_name);
                                    }
                                } else {
                                    set.insert(tag.id);
                                }
                                filter_changed = true;
                            }
                        }
                    });
                }
            }

            // Extra filters row
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Extras:")
                        .small()
                        .color(egui::Color32::GRAY),
                );
                if ui
                    .selectable_label(self.filter_audio, "Nur mit Audio")
                    .clicked()
                {
                    self.filter_audio = !self.filter_audio;
                    filter_changed = true;
                }
                if ui
                    .selectable_label(self.filter_untagged, "Ohne Tags")
                    .clicked()
                {
                    self.filter_untagged = !self.filter_untagged;
                    filter_changed = true;
                }
            });

            if filter_changed {
                self.refresh_songs_only();
            }

            ui.add_space(4.0);

            // Toolbar — result count + sort
            let mut sort_changed = false;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} Songs gefunden", self.songs.len()))
                        .color(egui::Color32::GRAY),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let current_label = self.sort_mode.label();
                    egui::ComboBox::from_label("Sortierung")
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                            for mode in SortMode::all() {
                                if ui
                                    .selectable_value(&mut self.sort_mode, mode.clone(), mode.label())
                                    .changed()
                                {
                                    sort_changed = true;
                                }
                            }
                        });
                });
            });

            if sort_changed {
                self.refresh_songs_only();
            }

            ui.separator();

            // Song list
            let mut action: Option<SongAction> = None;

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.songs.is_empty() {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                egui::RichText::new("Keine Songs gefunden.")
                                    .color(egui::Color32::GRAY),
                            );
                        });
                        return;
                    }

                    for song in &self.songs {
                        egui::Frame::group(ui.style())
                            .inner_margin(8.0)
                            .rounding(6.0)
                            .show(ui, |ui| {
                                // Title + artist + audio
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&song.titel).strong());
                                    if let Some(ref artist) = song.artist {
                                        ui.label(
                                            egui::RichText::new(format!("— {artist}"))
                                                .color(egui::Color32::GRAY),
                                        );
                                    }
                                    if song.has_audio {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new("Audio")
                                                        .small()
                                                        .color(egui::Color32::from_rgb(45, 106, 79)),
                                                );
                                            },
                                        );
                                    }
                                });

                                // Tags
                                if !song.tags.is_empty() {
                                    ui.horizontal_wrapped(|ui| {
                                        for tag in &song.tags {
                                            let color = tag_color(&tag.kategorie);
                                            let text = egui::RichText::new(&tag.wert)
                                                .small()
                                                .color(egui::Color32::WHITE);

                                            let button = egui::Button::new(text)
                                                .fill(color)
                                                .rounding(12.0)
                                                .small();

                                            let resp = ui.add(button);
                                            if resp.clicked() {
                                                action = Some(SongAction::ConfirmRemoveTag {
                                                    song_id: song.id,
                                                    tag_id: tag.id,
                                                    tag_wert: tag.wert.clone(),
                                                });
                                            }
                                            if resp.hovered() {
                                                resp.on_hover_text(if tag.auto_generated {
                                                    "Automatisch — Klick zum Entfernen"
                                                } else {
                                                    "Manuell — Klick zum Entfernen"
                                                });
                                            }
                                        }
                                    });
                                }

                                // Path + actions
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(&song.dateipfad)
                                            .small()
                                            .color(egui::Color32::DARK_GRAY),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("Datei öffnen").clicked() {
                                                action = Some(SongAction::OpenFile(
                                                    song.dateipfad.clone(),
                                                ));
                                            }
                                            if ui.small_button("Bearbeiten").clicked() {
                                                action = Some(SongAction::Edit {
                                                    song_id: song.id,
                                                    titel: song.titel.clone(),
                                                    artist: song
                                                        .artist
                                                        .clone()
                                                        .unwrap_or_default(),
                                                });
                                            }
                                            if ui.small_button("+ Tag").clicked() {
                                                action = Some(SongAction::OpenTagModal {
                                                    song_id: song.id,
                                                    song_titel: song.titel.clone(),
                                                });
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(2.0);
                    }
                });

            // Process actions
            if let Some(act) = action {
                match act {
                    SongAction::OpenFile(rel_path) => {
                        let full_path = self.base_dir.join(&rel_path);
                        let _ = std::process::Command::new("open")
                            .arg(&full_path)
                            .spawn();
                    }
                    SongAction::OpenTagModal { song_id, song_titel } => {
                        self.tag_modal = Some(TagModalState {
                            song_id,
                            song_titel,
                            kategorie_idx: 0,
                            wert: String::new(),
                        });
                    }
                    SongAction::Edit {
                        song_id,
                        titel,
                        artist,
                    } => {
                        self.edit_modal = Some(EditModalState {
                            song_id,
                            titel,
                            artist,
                        });
                    }
                    SongAction::ConfirmRemoveTag {
                        song_id,
                        tag_id,
                        tag_wert,
                    } => {
                        self.confirm_remove = Some(ConfirmRemoveTag {
                            song_id,
                            tag_id,
                            tag_wert,
                        });
                    }
                }
            }
        });

        // Tag modal
        let mut close_tag_modal = false;
        if let Some(ref mut modal) = self.tag_modal {
            let mut open = true;
            egui::Window::new(format!("Tag hinzufügen — {}", modal.song_titel))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .fixed_size([350.0, 300.0])
                .show(ctx, |ui| {
                    let categories = ["instrument", "schwierigkeit", "stil", "technik", "stimmung", "kapo"];
                    let labels = ["Instrument", "Schwierigkeit", "Stil", "Technik", "Stimmung", "Kapo"];

                    ui.horizontal(|ui| {
                        ui.label("Kategorie:");
                        egui::ComboBox::from_id_salt("tag_kategorie")
                            .selected_text(labels[modal.kategorie_idx])
                            .show_ui(ui, |ui| {
                                for (i, label) in labels.iter().enumerate() {
                                    ui.selectable_value(&mut modal.kategorie_idx, i, *label);
                                }
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Wert:");
                        let response = ui.text_edit_singleline(&mut modal.wert);
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if !modal.wert.trim().is_empty() {
                                let conn = self.db.lock().unwrap();
                                add_tag_to_song(
                                    &conn,
                                    modal.song_id,
                                    categories[modal.kategorie_idx],
                                    modal.wert.trim(),
                                );
                                drop(conn);
                                self.needs_refresh = true;
                                close_tag_modal = true;
                            }
                        }
                    });

                    if ui.button("Hinzufügen").clicked() && !modal.wert.trim().is_empty() {
                        let conn = self.db.lock().unwrap();
                        add_tag_to_song(
                            &conn,
                            modal.song_id,
                            categories[modal.kategorie_idx],
                            modal.wert.trim(),
                        );
                        drop(conn);
                        self.needs_refresh = true;
                        close_tag_modal = true;
                    }

                    ui.separator();
                    ui.label(egui::RichText::new("Vorhandene Tags:").small());

                    // Show existing tags as quick-add buttons
                    let song_tag_ids: HashSet<i64> = {
                        if let Some(song) = self.songs.iter().find(|s| s.id == modal.song_id) {
                            song.tags.iter().map(|t| t.id).collect()
                        } else {
                            HashSet::new()
                        }
                    };

                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let mut added = false;
                                for group in &self.tags {
                                    for tag in &group.tags {
                                        if song_tag_ids.contains(&tag.id) {
                                            continue;
                                        }
                                        let color = tag_color(&group.kategorie);
                                        let btn = egui::Button::new(
                                            egui::RichText::new(&tag.wert)
                                                .small()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(color)
                                        .rounding(12.0)
                                        .small();

                                        if ui.add(btn).clicked() {
                                            let conn = self.db.lock().unwrap();
                                            add_tag_to_song(
                                                &conn,
                                                modal.song_id,
                                                &group.kategorie,
                                                &tag.wert,
                                            );
                                            drop(conn);
                                            added = true;
                                        }
                                    }
                                }
                                if added {
                                    self.needs_refresh = true;
                                    close_tag_modal = true;
                                }
                            });
                        });
                });

            if !open {
                close_tag_modal = true;
            }
        }
        if close_tag_modal {
            self.tag_modal = None;
            if self.needs_refresh {
                self.refresh_data();
                self.needs_refresh = false;
            }
        }

        // Edit modal
        let mut close_edit_modal = false;
        let mut save_edit = false;
        if let Some(ref mut modal) = self.edit_modal {
            let mut open = true;
            egui::Window::new("Song bearbeiten")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .fixed_size([350.0, 150.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Titel:");
                        ui.text_edit_singleline(&mut modal.titel);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Artist:");
                        ui.text_edit_singleline(&mut modal.artist);
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Speichern").clicked() {
                            save_edit = true;
                        }
                        if ui.button("Abbrechen").clicked() {
                            close_edit_modal = true;
                        }
                    });
                });

            if !open {
                close_edit_modal = true;
            }
        }
        if save_edit {
            if let Some(modal) = &self.edit_modal {
                let conn = self.db.lock().unwrap();
                update_song(&conn, modal.song_id, &modal.titel, &modal.artist);
                drop(conn);
                self.refresh_data();
            }
            self.edit_modal = None;
        } else if close_edit_modal {
            self.edit_modal = None;
        }

        // Confirm remove tag dialog
        let mut close_confirm = false;
        let mut do_remove = false;
        if let Some(ref confirm) = self.confirm_remove {
            let mut open = true;
            egui::Window::new("Tag entfernen?")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .fixed_size([300.0, 80.0])
                .show(ctx, |ui| {
                    ui.label(format!("Tag \"{}\" entfernen?", confirm.tag_wert));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Entfernen").clicked() {
                            do_remove = true;
                        }
                        if ui.button("Abbrechen").clicked() {
                            close_confirm = true;
                        }
                    });
                });

            if !open {
                close_confirm = true;
            }
        }
        if do_remove {
            if let Some(confirm) = &self.confirm_remove {
                let conn = self.db.lock().unwrap();
                remove_tag_from_song(&conn, confirm.song_id, confirm.tag_id);
                drop(conn);
                self.refresh_data();
            }
            self.confirm_remove = None;
        } else if close_confirm {
            self.confirm_remove = None;
        }
    }
}

enum SongAction {
    OpenFile(String),
    OpenTagModal { song_id: i64, song_titel: String },
    Edit { song_id: i64, titel: String, artist: String },
    ConfirmRemoveTag { song_id: i64, tag_id: i64, tag_wert: String },
}
