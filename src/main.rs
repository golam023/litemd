// LiteMD — a Sumatra-PDF-style lightweight, native, dependency-free Markdown viewer.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MAX_RECENT: usize = 10;

#[derive(Serialize, Deserialize, Default)]
struct Settings {
    recent_files: Vec<PathBuf>,
    dark_mode: bool,
    zoom: f32,
}

impl Settings {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("LiteMD").join("settings.json"))
    }

    fn load() -> Self {
        if let Some(p) = Self::path() {
            if let Ok(data) = std::fs::read_to_string(&p) {
                if let Ok(s) = serde_json::from_str::<Settings>(&data) {
                    return s;
                }
            }
        }
        Settings {
            recent_files: Vec::new(),
            dark_mode: true,
            zoom: 1.0,
        }
    }

    fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(data) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, data);
            }
        }
    }

    fn push_recent(&mut self, path: &Path) {
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.to_path_buf());
        self.recent_files.truncate(MAX_RECENT);
        self.save();
    }
}

struct LiteMdApp {
    settings: Settings,
    current_path: Option<PathBuf>,
    content: String,
    cache: CommonMarkCache,
    search_query: String,
    search_open: bool,
    last_mtime: Option<SystemTime>,
    watch_timer: f32,
    status: String,
    show_about: bool,
}

impl LiteMdApp {
    fn new(cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>) -> Self {
        let settings = Settings::load();

        let visuals = if settings.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        cc.egui_ctx.set_visuals(visuals);
        cc.egui_ctx.set_pixels_per_point(settings.zoom.max(0.6));

        let mut app = Self {
            settings,
            current_path: None,
            content: String::new(),
            cache: CommonMarkCache::default(),
            search_query: String::new(),
            search_open: false,
            last_mtime: None,
            watch_timer: 0.0,
            status: "Ready".to_owned(),
            show_about: false,
        };

        if let Some(path) = initial_path {
            app.open_path(path);
        }

        app
    }

    fn open_path(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.content = text;
                self.last_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                self.status = format!("Opened {}", path.display());
                self.settings.push_recent(&path);
                self.current_path = Some(path);
            }
            Err(e) => {
                self.status = format!("Failed to open {}: {e}", path.display());
            }
        }
    }

    fn pick_and_open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Markdown", &["md", "markdown", "mdown", "mkd", "mkdn", "txt"])
            .pick_file()
        {
            self.open_path(path);
        }
    }

    fn check_for_external_changes(&mut self, dt: f32) {
        self.watch_timer += dt;
        if self.watch_timer < 1.0 {
            return;
        }
        self.watch_timer = 0.0;
        if let Some(path) = self.current_path.clone() {
            if let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) {
                if Some(modified) != self.last_mtime {
                    self.open_path(path);
                    self.status = "Reloaded (file changed on disk)".to_owned();
                }
            }
        }
    }

    fn toggle_theme(&mut self, ctx: &egui::Context) {
        self.settings.dark_mode = !self.settings.dark_mode;
        ctx.set_visuals(if self.settings.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        self.settings.save();
    }

    fn set_zoom(&mut self, ctx: &egui::Context, zoom: f32) {
        self.settings.zoom = zoom.clamp(0.6, 3.0);
        ctx.set_pixels_per_point(self.settings.zoom);
        self.settings.save();
    }

    fn set_as_default_app(&mut self) {
        #[cfg(windows)]
        {
            match register_default_handler() {
                Ok(_) => self.status = "Registered. Right-click a .md file -> Open with -> LiteMD, or Settings -> Default apps -> .md.".to_owned(),
                Err(e) => self.status = format!("Registration failed: {e}"),
            }
        }
        #[cfg(not(windows))]
        {
            self.status = "Default-app registration is only available in the Windows build.".to_owned();
        }
    }

    fn take_pending_action(&mut self) -> Option<PendingAction> {
        if let Some(rest) = self.status.strip_prefix("__drop__") {
            return Some(PendingAction::OpenPath(PathBuf::from(rest)));
        }
        match self.status.as_str() {
            "__open__" => Some(PendingAction::PickAndOpen),
            "__zoom_in__" => Some(PendingAction::ZoomBy(0.1)),
            "__zoom_out__" => Some(PendingAction::ZoomBy(-0.1)),
            "__zoom_reset__" => Some(PendingAction::ZoomReset),
            _ => None,
        }
    }
}

enum PendingAction {
    OpenPath(PathBuf),
    PickAndOpen,
    ZoomBy(f32),
    ZoomReset,
}

impl eframe::App for LiteMdApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.stable_dt);
        self.check_for_external_changes(dt);
        ctx.request_repaint_after(Duration::from_millis(500));

        // Drag & drop
        let dropped = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
        if let Some(p) = dropped {
            self.status = format!("__drop__{}", p.display());
        }

        // Keyboard shortcuts
        ctx.input(|i| {
            if i.modifiers.ctrl && i.key_pressed(egui::Key::O) {
                self.status = "__open__".to_owned();
            }
            if i.modifiers.ctrl && i.key_pressed(egui::Key::F) {
                self.search_open = !self.search_open;
            }
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Plus) {
                self.status = "__zoom_in__".to_owned();
            }
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Minus) {
                self.status = "__zoom_out__".to_owned();
            }
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Num0) {
                self.status = "__zoom_reset__".to_owned();
            }
        });

        if let Some(action) = self.take_pending_action() {
            self.status = "Ready".to_owned();
            match action {
                PendingAction::OpenPath(p) => self.open_path(p),
                PendingAction::PickAndOpen => self.pick_and_open(),
                PendingAction::ZoomBy(delta) => {
                    let z = self.settings.zoom;
                    self.set_zoom(ctx, z + delta);
                }
                PendingAction::ZoomReset => self.set_zoom(ctx, 1.0),
            }
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open... (Ctrl+O)").clicked() {
                        self.pick_and_open();
                        ui.close_menu();
                    }
                    ui.menu_button("Recent Files", |ui| {
                        if self.settings.recent_files.is_empty() {
                            ui.label("(empty)");
                        } else {
                            let recents = self.settings.recent_files.clone();
                            for p in recents {
                                if ui.button(p.display().to_string()).clicked() {
                                    self.open_path(p);
                                    ui.close_menu();
                                }
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Set LiteMD as default .md app").clicked() {
                        self.set_as_default_app();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("View", |ui| {
                    let theme_label = if self.settings.dark_mode { "Switch to Light Theme" } else { "Switch to Dark Theme" };
                    if ui.button(theme_label).clicked() {
                        self.toggle_theme(ctx);
                        ui.close_menu();
                    }
                    if ui.button("Zoom In (Ctrl++)").clicked() {
                        let z = self.settings.zoom;
                        self.set_zoom(ctx, z + 0.1);
                    }
                    if ui.button("Zoom Out (Ctrl+-)").clicked() {
                        let z = self.settings.zoom;
                        self.set_zoom(ctx, z - 0.1);
                    }
                    if ui.button("Reset Zoom (Ctrl+0)").clicked() {
                        self.set_zoom(ctx, 1.0);
                    }
                    let search_label = if self.search_open { "Hide Search (Ctrl+F)" } else { "Find (Ctrl+F)" };
                    if ui.button(search_label).clicked() {
                        self.search_open = !self.search_open;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About LiteMD").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(p) = &self.current_path {
                        ui.label(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                    }
                });
            });
        });

        // Search bar
        if self.search_open {
            egui::TopBottomPanel::top("search_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Find:");
                    ui.text_edit_singleline(&mut self.search_query);
                    if !self.search_query.is_empty() {
                        let count = self
                            .content
                            .to_lowercase()
                            .matches(self.search_query.to_lowercase().as_str())
                            .count();
                        ui.label(format!("{count} match(es)"));
                    }
                    if ui.button("Close").clicked() {
                        self.search_open = false;
                        self.search_query.clear();
                    }
                });
            });
        }

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(self.status.clone());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("Zoom: {:.0}%", self.settings.zoom * 100.0));
                });
            });
        });

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.content.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label("Drag & drop a .md file here, or File -> Open (Ctrl+O)");
                });
                return;
            }

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(900.0));

                if self.search_query.is_empty() {
                    CommonMarkViewer::new().show(ui, &mut self.cache, &self.content);
                } else {
                    // Predictable line-based highlight while searching, independent of markdown structure.
                    let query = self.search_query.to_lowercase();
                    for line in self.content.lines() {
                        if !query.is_empty() && line.to_lowercase().contains(&query) {
                            ui.colored_label(egui::Color32::YELLOW, line);
                        } else {
                            ui.label(line);
                        }
                    }
                }
            });
        });

        // About dialog
        if self.show_about {
            egui::Window::new("About LiteMD")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("LiteMD - a lightweight, portable Markdown viewer.");
                    ui.label("No installer. No runtime dependencies. Single .exe.");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    if ui.button("Close").clicked() {
                        self.show_about = false;
                    }
                });
        }
    }
}

#[cfg(windows)]
fn register_default_handler() -> std::io::Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let exe_path = std::env::current_exe()?;
    let exe_str = exe_path.to_string_lossy().to_string();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    let (progid, _) = hkcu.create_subkey("Software\\Classes\\LiteMD.md")?;
    progid.set_value("", &"LiteMD Markdown Document")?;
    let (icon_key, _) = progid.create_subkey("DefaultIcon")?;
    icon_key.set_value("", &format!("{exe_str},0"))?;
    let (cmd_key, _) = progid.create_subkey("shell\\open\\command")?;
    cmd_key.set_value("", &format!("\"{exe_str}\" \"%1\""))?;

    let (caps, _) = hkcu.create_subkey("Software\\LiteMD\\Capabilities")?;
    caps.set_value("ApplicationName", &"LiteMD")?;
    caps.set_value("ApplicationDescription", &"Lightweight Markdown Viewer")?;
    let (file_assoc, _) = caps.create_subkey("FileAssociations")?;
    for ext in [".md", ".markdown", ".mdown", ".mkd", ".mkdn"] {
        file_assoc.set_value(ext, &"LiteMD.md")?;
    }

    let (registered_apps, _) = hkcu.create_subkey("Software\\RegisteredApplications")?;
    registered_apps.set_value("LiteMD", &"Software\\LiteMD\\Capabilities")?;

    for ext in [".md", ".markdown", ".mdown", ".mkd", ".mkdn"] {
        let key_path = format!("Software\\Classes\\{ext}");
        let (ext_key, _) = hkcu.create_subkey(&key_path)?;
        ext_key.set_value("", &"LiteMD.md")?;
    }

    Ok(())
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    let initial_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .filter(|p| p.exists());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 720.0])
            .with_min_inner_size([420.0, 300.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "LiteMD",
        options,
        Box::new(move |cc| Ok(Box::new(LiteMdApp::new(cc, initial_path)))),
    )
}
