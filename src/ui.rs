use crate::config::{save_config, Config};
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

// --- Color Palette ---
mod palette {
    use eframe::egui::Color32;

    pub const BG_DEEP: Color32 = Color32::from_rgb(22, 20, 26);
    pub const BG_SURFACE: Color32 = Color32::from_rgb(32, 30, 38);
    pub const BG_CARD: Color32 = Color32::from_rgb(38, 36, 46);
    pub const BG_INPUT: Color32 = Color32::from_rgb(18, 16, 22);
    pub const BG_HEADER: Color32 = Color32::from_rgb(26, 24, 32);

    pub const ACCENT: Color32 = Color32::from_rgb(235, 180, 60);
    pub const ACCENT_DIM: Color32 = Color32::from_rgb(180, 138, 48);
    pub const ACCENT_RED: Color32 = Color32::from_rgb(220, 75, 85);

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(242, 238, 230);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(175, 170, 162);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(120, 115, 108);

    pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(52, 48, 62);
    pub const BORDER_ACTIVE: Color32 = Color32::from_rgb(80, 75, 95);

    pub const TAG_INSTRUMENT: Color32 = Color32::from_rgb(38, 148, 88);
    pub const TAG_SCHWIERIGKEIT: Color32 = Color32::from_rgb(210, 135, 48);
    pub const TAG_STIL: Color32 = Color32::from_rgb(58, 125, 190);
    pub const TAG_TECHNIK: Color32 = Color32::from_rgb(138, 95, 175);
    pub const TAG_ARTIST: Color32 = Color32::from_rgb(188, 118, 52);
    pub const TAG_STIMMUNG: Color32 = Color32::from_rgb(95, 142, 78);
    pub const TAG_KAPO: Color32 = Color32::from_rgb(132, 128, 148);

    pub const AUDIO_GREEN: Color32 = Color32::from_rgb(85, 195, 130);

    pub const BTN_BG: Color32 = Color32::from_rgb(48, 45, 58);
    pub const BTN_HOVER: Color32 = Color32::from_rgb(62, 58, 74);
}

fn tag_color(kategorie: &str) -> egui::Color32 {
    match kategorie {
        "instrument" => palette::TAG_INSTRUMENT,
        "schwierigkeit" => palette::TAG_SCHWIERIGKEIT,
        "stil" => palette::TAG_STIL,
        "technik" => palette::TAG_TECHNIK,
        "artist" => palette::TAG_ARTIST,
        "stimmung" => palette::TAG_STIMMUNG,
        "kapo" => palette::TAG_KAPO,
        _ => palette::TEXT_MUTED,
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

    // Settings
    show_settings: bool,
    filters_open: bool,

    // Audio playback
    audio_process: Option<std::process::Child>,
    audio_playing_song_id: Option<i64>,

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
            show_settings: false,
            filters_open: true,
            audio_process: None,
            audio_playing_song_id: None,
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

    fn stop_audio(&mut self) {
        if let Some(ref mut child) = self.audio_process {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.audio_process = None;
        self.audio_playing_song_id = None;
    }

    fn play_audio(&mut self, song_id: i64, audio_pfad: &str) {
        self.stop_audio();
        let full_path = self.base_dir.join(audio_pfad);
        match std::process::Command::new("afplay")
            .arg(&full_path)
            .spawn()
        {
            Ok(child) => {
                self.audio_process = Some(child);
                self.audio_playing_song_id = Some(song_id);
            }
            Err(_) => {
                let _ = std::process::Command::new("open")
                    .arg(&full_path)
                    .spawn();
            }
        }
    }

    fn check_audio_finished(&mut self) {
        if let Some(ref mut child) = self.audio_process {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.audio_process = None;
                    self.audio_playing_song_id = None;
                }
                _ => {}
            }
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = palette::BG_DEEP;
        visuals.window_fill = palette::BG_SURFACE;
        visuals.extreme_bg_color = palette::BG_INPUT;
        visuals.faint_bg_color = palette::BG_CARD;

        visuals.widgets.noninteractive.bg_fill = palette::BG_SURFACE;
        visuals.widgets.noninteractive.fg_stroke =
            egui::Stroke::new(1.0, palette::TEXT_SECONDARY);
        visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(0.5, palette::BORDER_SUBTLE);
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(6.0);

        visuals.widgets.inactive.bg_fill = palette::BTN_BG;
        visuals.widgets.inactive.fg_stroke =
            egui::Stroke::new(1.0, palette::TEXT_PRIMARY);
        visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(0.5, palette::BORDER_SUBTLE);
        visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);

        visuals.widgets.hovered.bg_fill = palette::BTN_HOVER;
        visuals.widgets.hovered.fg_stroke =
            egui::Stroke::new(1.0, palette::TEXT_PRIMARY);
        visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0, palette::BORDER_ACTIVE);
        visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);

        visuals.widgets.active.bg_fill = palette::ACCENT_DIM;
        visuals.widgets.active.fg_stroke =
            egui::Stroke::new(1.0, palette::TEXT_PRIMARY);
        visuals.widgets.active.bg_stroke =
            egui::Stroke::new(1.0, palette::ACCENT);
        visuals.widgets.active.rounding = egui::Rounding::same(6.0);

        visuals.selection.bg_fill = egui::Color32::from_rgba_premultiplied(235, 180, 60, 40);
        visuals.selection.stroke = egui::Stroke::new(1.0, palette::ACCENT);

        visuals.window_rounding = egui::Rounding::same(10.0);
        visuals.window_shadow = egui::epaint::Shadow {
            offset: egui::vec2(0.0, 4.0),
            blur: 16.0,
            spread: 2.0,
            color: egui::Color32::from_black_alpha(100),
        };
        visuals.window_stroke = egui::Stroke::new(1.0, palette::BORDER_SUBTLE);
        visuals.popup_shadow = egui::epaint::Shadow {
            offset: egui::vec2(0.0, 2.0),
            blur: 8.0,
            spread: 1.0,
            color: egui::Color32::from_black_alpha(80),
        };

        visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);

        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        use egui::TextStyle;
        style
            .text_styles
            .insert(TextStyle::Small, egui::FontId::proportional(12.5));
        style
            .text_styles
            .insert(TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Heading, egui::FontId::proportional(24.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, egui::FontId::monospace(13.5));
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.window_margin = egui::Margin::same(16.0);
        ctx.set_style(style);
    }
}

fn styled_small_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .size(12.5)
            .color(palette::TEXT_SECONDARY),
    )
    .fill(palette::BTN_BG)
    .stroke(egui::Stroke::new(0.5, palette::BORDER_SUBTLE))
    .rounding(5.0);
    ui.add(btn)
}

fn stat_badge(ui: &mut egui::Ui, value: &str, label: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        ui.label(
            egui::RichText::new(value)
                .size(14.0)
                .strong()
                .color(color),
        );
        ui.label(
            egui::RichText::new(label)
                .size(12.0)
                .color(palette::TEXT_MUTED),
        );
    });
}

impl eframe::App for SongIndexApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while self.watcher_rx.try_recv().is_ok() {
            self.needs_refresh = true;
        }
        if self.needs_refresh {
            self.needs_refresh = false;
            self.refresh_data();
        }

        self.check_audio_finished();
        if self.audio_playing_song_id.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        self.apply_theme(ctx);

        // ── Header ──
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(palette::BG_HEADER)
                    .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                    .stroke(egui::Stroke::new(0.5, palette::BORDER_SUBTLE)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Songindex")
                            .size(22.0)
                            .strong()
                            .color(palette::TEXT_PRIMARY),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("\u{266B}")
                            .size(18.0)
                            .color(palette::ACCENT_DIM),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let gear = egui::Button::new(
                            egui::RichText::new("\u{2699}")
                                .size(18.0)
                                .color(palette::TEXT_MUTED),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(egui::Stroke::NONE);
                        if ui.add(gear).on_hover_text("Einstellungen").clicked() {
                            self.show_settings = !self.show_settings;
                        }

                        ui.add_space(8.0);
                        stat_badge(
                            ui,
                            &self.stats.untagged_songs.to_string(),
                            "ohne Tags",
                            palette::TEXT_MUTED,
                        );
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("\u{00B7}")
                                .size(16.0)
                                .color(palette::BORDER_SUBTLE),
                        );
                        ui.add_space(6.0);
                        stat_badge(
                            ui,
                            &self.stats.songs_with_audio.to_string(),
                            "mit Audio",
                            palette::AUDIO_GREEN,
                        );
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("\u{00B7}")
                                .size(16.0)
                                .color(palette::BORDER_SUBTLE),
                        );
                        ui.add_space(6.0);
                        stat_badge(
                            ui,
                            &self.stats.total_songs.to_string(),
                            "Songs",
                            palette::ACCENT,
                        );
                    });
                });
            });

        // ── Central Panel ──
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette::BG_DEEP)
                    .inner_margin(egui::Margin::symmetric(16.0, 12.0)),
            )
            .show(ctx, |ui| {
                // ── Search bar ──
                let mut search_changed = false;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    ui.label(
                        egui::RichText::new("\u{1F50D}")
                            .size(15.0)
                            .color(palette::TEXT_MUTED),
                    );

                    let search_width = ui.available_width() - 80.0;
                    let response = ui.add_sized(
                        [search_width, 28.0],
                        egui::TextEdit::singleline(&mut self.search_text)
                            .hint_text(
                                egui::RichText::new("Suche nach Titel oder Artist...")
                                    .color(palette::TEXT_MUTED),
                            )
                            .text_color(palette::TEXT_PRIMARY)
                            .margin(egui::Margin::symmetric(8.0, 4.0)),
                    );
                    if response.changed() {
                        search_changed = true;
                    }

                    let rescan_btn = egui::Button::new(
                        egui::RichText::new("Rescan")
                            .size(13.0)
                            .color(palette::TEXT_SECONDARY),
                    )
                    .fill(palette::BTN_BG)
                    .rounding(6.0);
                    if ui.add(rescan_btn).clicked() {
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

                // ── Filters accordion ──
                let mut filter_changed = false;

                // Accordion header
                let active_count: usize = self.active_filters.values().map(|s| s.len()).sum::<usize>()
                    + if self.filter_audio { 1 } else { 0 }
                    + if self.filter_untagged { 1 } else { 0 };

                ui.horizontal(|ui| {
                    let arrow = if self.filters_open { "\u{25BE}" } else { "\u{25B8}" };
                    let header_text = if active_count > 0 {
                        format!("{} Filter ({})", arrow, active_count)
                    } else {
                        format!("{} Filter", arrow)
                    };

                    let header_btn = egui::Button::new(
                        egui::RichText::new(&header_text)
                            .size(13.0)
                            .color(if active_count > 0 {
                                palette::ACCENT
                            } else {
                                palette::TEXT_SECONDARY
                            }),
                    )
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE);

                    if ui.add(header_btn).clicked() {
                        self.filters_open = !self.filters_open;
                    }
                });

                if self.filters_open {
                    ui.add_space(2.0);

                    for cat_name in CATEGORY_ORDER {
                        let group = self.tags.iter().find(|g| g.kategorie == *cat_name);
                        if let Some(group) = group {
                            if group.tags.is_empty() {
                                continue;
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}:",
                                        category_label(cat_name)
                                    ))
                                    .size(12.5)
                                    .color(palette::TEXT_MUTED),
                                );
                                ui.add_space(2.0);

                                for tag in &group.tags {
                                    let is_active = self
                                        .active_filters
                                        .get(*cat_name)
                                        .map_or(false, |s| s.contains(&tag.id));

                                    let text = format!("{} ({})", tag.wert, tag.count);
                                    let label = if is_active {
                                        egui::RichText::new(&text)
                                            .size(12.5)
                                            .color(palette::TEXT_PRIMARY)
                                    } else {
                                        egui::RichText::new(&text)
                                            .size(12.5)
                                            .color(palette::TEXT_SECONDARY)
                                    };

                                    let response = ui.selectable_label(is_active, label);

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

                    // Extra filters
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                        ui.label(
                            egui::RichText::new("Extras:")
                                .size(12.5)
                                .color(palette::TEXT_MUTED),
                        );
                        ui.add_space(2.0);

                        let audio_label = if self.filter_audio {
                            egui::RichText::new("Nur mit Audio")
                                .size(12.5)
                                .color(palette::AUDIO_GREEN)
                        } else {
                            egui::RichText::new("Nur mit Audio")
                                .size(12.5)
                                .color(palette::TEXT_SECONDARY)
                        };
                        if ui
                            .selectable_label(self.filter_audio, audio_label)
                            .clicked()
                        {
                            self.filter_audio = !self.filter_audio;
                            filter_changed = true;
                        }

                        let untagged_label = if self.filter_untagged {
                            egui::RichText::new("Ohne Tags")
                                .size(12.5)
                                .color(palette::ACCENT)
                        } else {
                            egui::RichText::new("Ohne Tags")
                                .size(12.5)
                                .color(palette::TEXT_SECONDARY)
                        };
                        if ui
                            .selectable_label(self.filter_untagged, untagged_label)
                            .clicked()
                        {
                            self.filter_untagged = !self.filter_untagged;
                            filter_changed = true;
                        }
                    });
                }

                if filter_changed {
                    self.refresh_songs_only();
                }

                ui.add_space(8.0);

                // ── Toolbar ──
                let mut sort_changed = false;
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{} Songs gefunden", self.songs.len()))
                            .size(14.0)
                            .color(palette::TEXT_SECONDARY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let current_label = self.sort_mode.label();
                        egui::ComboBox::from_label(
                            egui::RichText::new("Sortierung")
                                .size(12.5)
                                .color(palette::TEXT_MUTED),
                        )
                        .selected_text(
                            egui::RichText::new(current_label)
                                .size(13.0)
                                .color(palette::TEXT_SECONDARY),
                        )
                        .show_ui(ui, |ui| {
                            for mode in SortMode::all() {
                                if ui
                                    .selectable_value(
                                        &mut self.sort_mode,
                                        mode.clone(),
                                        mode.label(),
                                    )
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

                ui.add_space(4.0);

                // Thin separator line
                let rect = ui.available_rect_before_wrap();
                let sep_rect = egui::Rect::from_min_size(
                    rect.min,
                    egui::vec2(rect.width(), 1.0),
                );
                ui.painter()
                    .rect_filled(sep_rect, 0.0, palette::BORDER_SUBTLE);
                ui.add_space(6.0);

                // ── Song list ──
                let mut action: Option<SongAction> = None;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.songs.is_empty() {
                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("Keine Songs gefunden.")
                                        .size(16.0)
                                        .color(palette::TEXT_MUTED),
                                );
                            });
                            return;
                        }

                        for song in &self.songs {
                            egui::Frame::none()
                                .fill(palette::BG_CARD)
                                .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                                .rounding(8.0)
                                .stroke(egui::Stroke::new(0.5, palette::BORDER_SUBTLE))
                                .show(ui, |ui: &mut egui::Ui| {
                                    // Title row
                                    ui.horizontal(|ui: &mut egui::Ui| {
                                        ui.label(
                                            egui::RichText::new(&song.titel)
                                                .size(16.0)
                                                .strong()
                                                .color(palette::TEXT_PRIMARY),
                                        );
                                        if let Some(ref artist) = song.artist {
                                            ui.label(
                                                egui::RichText::new(format!("\u{2014} {artist}"))
                                                    .size(14.5)
                                                    .color(palette::TEXT_SECONDARY),
                                            );
                                        }
                                        if song.has_audio {
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui: &mut egui::Ui| {
                                                    let is_playing = self.audio_playing_song_id == Some(song.id);
                                                    let (label, bg_color) = if is_playing {
                                                        ("\u{25A0} Stop", palette::ACCENT_RED)
                                                    } else {
                                                        ("\u{25B6} Audio", palette::TAG_STIMMUNG)
                                                    };
                                                    let btn = egui::Button::new(
                                                        egui::RichText::new(label)
                                                            .size(11.5)
                                                            .color(egui::Color32::WHITE),
                                                    )
                                                    .fill(bg_color)
                                                    .rounding(4.0);
                                                    if ui.add(btn).clicked() {
                                                        if let Some(ref ap) = song.audio_pfad {
                                                            action = Some(SongAction::ToggleAudio {
                                                                song_id: song.id,
                                                                audio_pfad: ap.clone(),
                                                            });
                                                        }
                                                    }
                                                },
                                            );
                                        }
                                    });

                                    // Tags
                                    if !song.tags.is_empty() {
                                        ui.add_space(2.0);
                                        ui.horizontal_wrapped(|ui: &mut egui::Ui| {
                                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                                            for tag in &song.tags {
                                                let color = tag_color(&tag.kategorie);
                                                let text = egui::RichText::new(&tag.wert)
                                                    .size(11.5)
                                                    .color(egui::Color32::WHITE);

                                                let button = egui::Button::new(text)
                                                    .fill(color)
                                                    .rounding(10.0)
                                                    .stroke(egui::Stroke::NONE);

                                                let resp = ui.add(button);
                                                if resp.clicked() {
                                                    action =
                                                        Some(SongAction::ConfirmRemoveTag {
                                                            song_id: song.id,
                                                            tag_id: tag.id,
                                                            tag_wert: tag.wert.clone(),
                                                        });
                                                }
                                                if resp.hovered() {
                                                    resp.on_hover_text(if tag.auto_generated {
                                                        "Automatisch \u{2014} Klick zum Entfernen"
                                                    } else {
                                                        "Manuell \u{2014} Klick zum Entfernen"
                                                    });
                                                }
                                            }
                                        });
                                    }

                                    ui.add_space(2.0);

                                    // Path + actions
                                    ui.horizontal(|ui: &mut egui::Ui| {
                                        ui.label(
                                            egui::RichText::new(&song.dateipfad)
                                                .size(11.5)
                                                .color(palette::TEXT_MUTED),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui: &mut egui::Ui| {
                                                ui.spacing_mut().item_spacing.x = 4.0;
                                                if styled_small_button(ui, "Datei \u{00F6}ffnen")
                                                    .clicked()
                                                {
                                                    action = Some(SongAction::OpenFile(
                                                        song.dateipfad.clone(),
                                                    ));
                                                }
                                                if styled_small_button(ui, "Bearbeiten").clicked()
                                                {
                                                    action = Some(SongAction::Edit {
                                                        song_id: song.id,
                                                        titel: song.titel.clone(),
                                                        artist: song
                                                            .artist
                                                            .clone()
                                                            .unwrap_or_default(),
                                                    });
                                                }
                                                if styled_small_button(ui, "+ Tag").clicked() {
                                                    action = Some(SongAction::OpenTagModal {
                                                        song_id: song.id,
                                                        song_titel: song.titel.clone(),
                                                    });
                                                }
                                            },
                                        );
                                    });
                                });
                            ui.add_space(3.0);
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
                        SongAction::OpenTagModal {
                            song_id,
                            song_titel,
                        } => {
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
                        SongAction::ToggleAudio {
                            song_id,
                            audio_pfad,
                        } => {
                            if self.audio_playing_song_id == Some(song_id) {
                                self.stop_audio();
                            } else {
                                self.play_audio(song_id, &audio_pfad);
                            }
                        }
                    }
                }
            });

        // ── Settings window ──
        if self.show_settings {
            let mut open = true;
            egui::Window::new(
                egui::RichText::new("Einstellungen")
                    .size(16.0)
                    .color(palette::TEXT_PRIMARY),
            )
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size([450.0, 120.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Musikordner:")
                            .color(palette::TEXT_SECONDARY),
                    );
                    ui.label(
                        egui::RichText::new(self.base_dir.display().to_string())
                            .size(13.0)
                            .color(palette::TEXT_MUTED),
                    );
                });
                ui.add_space(8.0);
                if ui.button("Ordner \u{00E4}ndern").clicked() {
                    if let Some(new_dir) = rfd::FileDialog::new()
                        .set_title("Musikordner ausw\u{00E4}hlen")
                        .set_directory(&self.base_dir)
                        .pick_folder()
                    {
                        save_config(&Config {
                            music_dir: new_dir.clone(),
                        });
                        self.base_dir = new_dir;
                        let conn = self.db.lock().unwrap();
                        crate::scanner::scan_directory(&conn, &self.base_dir);
                        drop(conn);
                        self.refresh_data();
                    }
                }
            });
            if !open {
                self.show_settings = false;
            }
        }

        // ── Tag modal ──
        let mut close_tag_modal = false;
        if let Some(ref mut modal) = self.tag_modal {
            let mut open = true;
            egui::Window::new(
                egui::RichText::new(format!("Tag hinzuf\u{00FC}gen \u{2014} {}", modal.song_titel))
                    .size(15.0)
                    .color(palette::TEXT_PRIMARY),
            )
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size([380.0, 320.0])
            .show(ctx, |ui| {
                let categories =
                    ["instrument", "schwierigkeit", "stil", "technik", "stimmung", "kapo"];
                let labels =
                    ["Instrument", "Schwierigkeit", "Stil", "Technik", "Stimmung", "Kapo"];

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Kategorie:")
                            .color(palette::TEXT_SECONDARY),
                    );
                    egui::ComboBox::from_id_salt("tag_kategorie")
                        .selected_text(labels[modal.kategorie_idx])
                        .show_ui(ui, |ui| {
                            for (i, label) in labels.iter().enumerate() {
                                ui.selectable_value(&mut modal.kategorie_idx, i, *label);
                            }
                        });
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Wert:")
                            .color(palette::TEXT_SECONDARY),
                    );
                    let response = ui.text_edit_singleline(&mut modal.wert);
                    if response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
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

                ui.add_space(4.0);

                let add_btn = egui::Button::new(
                    egui::RichText::new("Hinzuf\u{00FC}gen")
                        .color(palette::TEXT_PRIMARY),
                )
                .fill(palette::ACCENT_DIM)
                .rounding(6.0);
                if ui.add(add_btn).clicked() && !modal.wert.trim().is_empty() {
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

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("Vorhandene Tags:")
                        .size(12.5)
                        .color(palette::TEXT_MUTED),
                );
                ui.add_space(4.0);

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
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                            let mut added = false;
                            for group in &self.tags {
                                for tag in &group.tags {
                                    if song_tag_ids.contains(&tag.id) {
                                        continue;
                                    }
                                    let color = tag_color(&group.kategorie);
                                    let btn = egui::Button::new(
                                        egui::RichText::new(&tag.wert)
                                            .size(11.5)
                                            .color(egui::Color32::WHITE),
                                    )
                                    .fill(color)
                                    .rounding(10.0)
                                    .stroke(egui::Stroke::NONE);

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

        // ── Edit modal ──
        let mut close_edit_modal = false;
        let mut save_edit = false;
        if let Some(ref mut modal) = self.edit_modal {
            let mut open = true;
            egui::Window::new(
                egui::RichText::new("Song bearbeiten")
                    .size(15.0)
                    .color(palette::TEXT_PRIMARY),
            )
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size([380.0, 160.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Titel:")
                            .color(palette::TEXT_SECONDARY),
                    );
                    ui.text_edit_singleline(&mut modal.titel);
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Artist:")
                            .color(palette::TEXT_SECONDARY),
                    );
                    ui.text_edit_singleline(&mut modal.artist);
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let save_btn = egui::Button::new(
                        egui::RichText::new("Speichern")
                            .color(palette::TEXT_PRIMARY),
                    )
                    .fill(palette::ACCENT_DIM)
                    .rounding(6.0);
                    if ui.add(save_btn).clicked() {
                        save_edit = true;
                    }
                    ui.add_space(4.0);
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

        // ── Confirm remove tag ──
        let mut close_confirm = false;
        let mut do_remove = false;
        if let Some(ref confirm) = self.confirm_remove {
            let mut open = true;
            egui::Window::new(
                egui::RichText::new("Tag entfernen?")
                    .size(15.0)
                    .color(palette::ACCENT_RED),
            )
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size([320.0, 90.0])
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Tag \u{201E}{}\u{201C} entfernen?",
                        confirm.tag_wert
                    ))
                    .color(palette::TEXT_PRIMARY),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let remove_btn = egui::Button::new(
                        egui::RichText::new("Entfernen")
                            .color(egui::Color32::WHITE),
                    )
                    .fill(palette::ACCENT_RED)
                    .rounding(6.0);
                    if ui.add(remove_btn).clicked() {
                        do_remove = true;
                    }
                    ui.add_space(4.0);
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
    ToggleAudio { song_id: i64, audio_pfad: String },
}
