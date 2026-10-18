use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use eframe::egui;

use crate::input::{self, Discovered};
use crate::mapping::{
    shape, AxisMode, ButtonMap, DestKey, InputState, OutAxis, Profile, DOF_NAMES,
};
use crate::output::PadIdentity;
use crate::pipeline::{Pipeline, Source};
use crate::profile;

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 660.0])
            .with_min_inner_size([700.0, 540.0]),
        ..Default::default()
    };
    eframe::run_native(
        "space-controller",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

pub struct App {
    state: Arc<Mutex<InputState>>,
    out_preview: Arc<Mutex<([f32; 6], Vec<u16>)>>,
    profile: Arc<RwLock<Profile>>,
    profile_names: Vec<String>,
    profile_sel: Option<String>,
    save_name: String,
    devices: Vec<Discovered>,
    device_sel: usize,
    pad_id: PadIdentity,
    theme: egui::ThemePreference,
    pipeline: Option<Pipeline>,
    status: String,
    capturing: Option<(usize, Option<u16>)>,
    /// Quick-assign state: target output axis + input baseline snapshot.
    assigning: Option<(OutAxis, [f32; 6], std::time::Instant)>,
    /// Result of asking the freedesktop appearance portal for color-scheme.
    /// None = couldn't detect; Some(true) = dark; Some(false) = light/none.
    system_dark: Option<bool>,
}

/// Ask xdg-desktop-portal for the desktop color scheme.
/// 1 = prefer-dark, 2 = prefer-light, 0/other = no preference.
fn detect_system_dark() -> Option<bool> {
    let out = std::process::Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply=literal",
            "--dest=org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings.Read",
            "string:org.freedesktop.appearance",
            "string:color-scheme",
        ])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let n = s
        .split_whitespace()
        .filter_map(|t| t.parse::<u32>().ok())
        .next_back()?;
    Some(n == 1)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Reserve layout space for scrollbars instead of floating over content.
        cc.egui_ctx
            .all_styles_mut(|s| s.spacing.scroll.floating = false);
        let profile_names = profile::list_profiles();
        let profile = profile_names
            .first()
            .and_then(|n| profile::load(n).ok())
            .unwrap_or_default();
        let mut app = Self {
            state: Arc::new(Mutex::new(InputState::default())),
            out_preview: Arc::new(Mutex::new(([0.0; 6], Vec::new()))),
            profile: Arc::new(RwLock::new(profile)),
            profile_sel: profile_names.first().cloned(),
            profile_names,
            save_name: String::new(),
            devices: input::find_devices(),
            device_sel: 0,
            pad_id: PadIdentity::Xbox360,
            theme: egui::ThemePreference::System,
            pipeline: None,
            status: "idle".into(),
            capturing: None,
            assigning: None,
            system_dark: detect_system_dark(),
        };
        if app.devices.is_empty() {
            app.status = "no 3Dconnexion device found — use Demo source".into();
        }
        app
    }

    fn start(&mut self) {
        if self.pipeline.is_some() {
            return;
        }
        let source = if let Some(dev) = self.devices.get(self.device_sel) {
            Source::Device(dev.clone())
        } else {
            Source::Demo
        };
        match Pipeline::start(
            source,
            self.pad_id,
            self.state.clone(),
            self.profile.clone(),
            self.out_preview.clone(),
        ) {
            Ok(p) => {
                self.status = match p.node_path() {
                    Some(n) => format!("running — output at {n}"),
                    None => "running".into(),
                };
                self.pipeline = Some(p);
            }
            Err(e) => {
                self.status = format!("start failed: {e:#}");
                eprintln!("[space-controller] {e:#}");
            }
        }
    }

    fn stop(&mut self) {
        if let Some(p) = self.pipeline.take() {
            p.stop();
            self.status = "stopped".into();
        }
    }
}

/// Accent color with enough contrast in both themes (egui's light-theme
/// selection color is too pale for bar fills).
fn accent(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        ui.visuals().selection.bg_fill
    } else {
        egui::Color32::from_rgb(37, 99, 235)
    }
}

fn axis_bar(ui: &mut egui::Ui, value: f32, width: f32) {
    let height = 12.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    let bg = ui.visuals().extreme_bg_color;
    painter.rect_filled(rect, 3.0, bg);
    let mid = rect.center().x;
    let half = rect.width() / 2.0 - 2.0;
    let end = mid + value.clamp(-1.0, 1.0) * half;
    let bar = egui::Rect::from_x_y_ranges(
        mid.min(end)..=mid.max(end),
        rect.top() + 2.0..=rect.bottom() - 2.0,
    );
    painter.rect_filled(bar, 2.0, accent(ui));
    painter.vline(
        mid,
        rect.y_range(),
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
    );
    // Value chip with its own background so the bar fill can't wash it out.
    let galley = painter.layout_no_wrap(
        format!("{:+.2}", value),
        egui::FontId::monospace(10.0),
        ui.visuals().text_color(),
    );
    let text_pos = egui::pos2(
        rect.right() - galley.size().x - 3.0,
        rect.center().y - galley.size().y / 2.0,
    );
    let chip = egui::Rect::from_min_size(
        text_pos - egui::vec2(2.0, 1.0),
        galley.size() + egui::vec2(4.0, 2.0),
    );
    painter.rect_filled(chip, 2.0, bg);
    painter.galley(text_pos, galley, ui.visuals().text_color());
}

/// Small response-curve preview: y = shape(map, x) over -1..1.
fn curve_plot(ui: &mut egui::Ui, map: &crate::mapping::AxisMap, raw: f32) {
    let size = egui::vec2(96.0, 48.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    let bg = ui.visuals().extreme_bg_color;
    let weak = ui.visuals().weak_text_color();
    painter.rect_filled(rect, 3.0, bg);
    painter.hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, weak),
    );
    painter.vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(1.0, weak),
    );
    let n = 48;
    let points: Vec<egui::Pos2> = (0..=n)
        .map(|i| {
            let x = i as f32 / n as f32 * 2.0 - 1.0;
            let y = shape(map, x);
            egui::pos2(
                rect.left() + (x + 1.0) / 2.0 * rect.width(),
                rect.center().y - y * (rect.height() / 2.0 - 3.0),
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.5, accent(ui)),
    ));
    // current position marker
    let cy = rect.center().y - shape(map, raw) * (rect.height() / 2.0 - 3.0);
    let cx = rect.left() + (raw.clamp(-1.0, 1.0) + 1.0) / 2.0 * rect.width();
    painter.circle_filled(
        egui::pos2(cx, cy),
        3.0,
        ui.visuals().strong_text_color(),
    );
}

fn stick_dot(
    painter: &egui::Painter,
    center: egui::Pos2,
    r: f32,
    x: f32,
    y: f32,
    weak: egui::Color32,
    accent: egui::Color32,
) {
    painter.circle_stroke(center, r, egui::Stroke::new(1.5, weak));
    painter.line_segment(
        [center - egui::vec2(r, 0.0), center + egui::vec2(r, 0.0)],
        egui::Stroke::new(1.0, weak),
    );
    painter.line_segment(
        [center - egui::vec2(0.0, r), center + egui::vec2(0.0, r)],
        egui::Stroke::new(1.0, weak),
    );
    painter.circle_filled(center + egui::vec2(x, -y) * (r - 6.0), 5.0, accent);
}

fn trigger_bar(ui: &mut egui::Ui, v: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 52.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    // triggers: -1 = released, +1 = full pull. Fill from bottom.
    let t = (v.clamp(-1.0, 1.0) + 1.0) / 2.0;
    let fill_h = rect.height() * t;
    painter.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - fill_h),
            rect.right_bottom(),
        ),
        3.0,
        accent(ui),
    );
}

fn gamepad_view(ui: &mut egui::Ui, out: &[f32; 6], pressed: &HashSet<u16>) {
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new("Virtual gamepad").strong());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("LT");
                trigger_bar(ui, out[OutAxis::Z.code() as usize]);
            });
            ui.vertical(|ui| {
                ui.label("LS");
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(52.0, 52.0), egui::Sense::hover());
                stick_dot(
                    ui.painter(),
                    rect.center(),
                    26.0,
                    out[OutAxis::X.code() as usize],
                    out[OutAxis::Y.code() as usize],
                    ui.visuals().weak_text_color(),
                    accent(ui),
                );
            });
            ui.vertical(|ui| {
                ui.label("RS");
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(52.0, 52.0), egui::Sense::hover());
                stick_dot(
                    ui.painter(),
                    rect.center(),
                    26.0,
                    out[OutAxis::Rx.code() as usize],
                    out[OutAxis::Ry.code() as usize],
                    ui.visuals().weak_text_color(),
                    accent(ui),
                );
            });
            ui.vertical(|ui| {
                ui.label("RT");
                trigger_bar(ui, out[OutAxis::Rz.code() as usize]);
            });
        });
        ui.separator();
        ui.label("buttons");
        let row1 = [
            DestKey::South,
            DestKey::East,
            DestKey::West,
            DestKey::North,
        ];
        let row2 = [
            DestKey::TL,
            DestKey::TR,
            DestKey::TL2,
            DestKey::TR2,
            DestKey::Select,
            DestKey::Start,
            DestKey::ThumbL,
            DestKey::ThumbR,
        ];
        for row in [&row1[..], &row2[..]] {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for k in row {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    let on = pressed.contains(&k.code());
                    ui.painter().circle(
                        rect.center(),
                        7.0,
                        if on {
                            accent(ui)
                        } else {
                            ui.visuals().extreme_bg_color
                        },
                        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        k.short_label(),
                        egui::FontId::proportional(7.5),
                        if on {
                            egui::Color32::WHITE
                        } else {
                            ui.visuals().text_color()
                        },
                    );
                }
            });
        }
    });
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint_after(Duration::from_millis(33));

        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal(|ui| {
                let running = self.pipeline.is_some();
                let (label, color) = if running {
                    ("Stop", egui::Color32::from_rgb(150, 40, 40))
                } else {
                    ("Start", egui::Color32::from_rgb(40, 120, 60))
                };
                let btn = egui::Button::new(
                    egui::RichText::new(label)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .fill(color);
                if ui.add(btn).clicked() {
                    if running {
                        self.stop();
                    } else {
                        self.start();
                    }
                }
                ui.separator();

                ui.add_enabled_ui(!running, |ui| {
                    let mut labels: Vec<String> =
                        self.devices.iter().map(|d| d.name.clone()).collect();
                    labels.push("Demo (synthetic input)".into());
                    egui::ComboBox::from_id_salt("source")
                        .selected_text(
                            labels
                                .get(self.device_sel)
                                .cloned()
                                .unwrap_or_else(|| "Demo (synthetic input)".into()),
                        )
                        .show_ui(ui, |ui| {
                            for (i, l) in labels.iter().enumerate() {
                                ui.selectable_value(&mut self.device_sel, i, l);
                            }
                        });
                    if ui.button("Refresh").clicked() {
                        self.devices = input::find_devices();
                    }
                    ui.separator();
                    ui.label("pad:");
                    egui::ComboBox::from_id_salt("padid")
                        .width(120.0)
                        .selected_text(self.pad_id.label())
                        .show_ui(ui, |ui| {
                            for id in PadIdentity::ALL {
                                ui.selectable_value(&mut self.pad_id, id, id.label());
                            }
                        });
                });
                ui.separator();
                ui.add(egui::Label::new(&self.status).truncate());
            });
        });

        let state = self.state.lock().unwrap().clone();
        let (out, out_pressed) = self.out_preview.lock().unwrap().clone();
        let pressed_set: HashSet<u16> = out_pressed.into_iter().collect();

        // Quick-assign: after arming an output axis, the first input DOF that
        // moves past the threshold gets bound to it.
        if let Some((dest, base, since)) = self.assigning {
            if since.elapsed() > Duration::from_secs(15) {
                self.assigning = None;
                self.status = "assign timed out".into();
            } else {
                let mut best = None;
                let mut best_d = 0.35f32;
                let mut live = (0usize, 0f32);
                for i in 0..6 {
                    let d = (state.axes[i] - base[i]).abs();
                    if d > live.1 {
                        live = (i, d);
                    }
                    if d > best_d {
                        best_d = d;
                        best = Some(i);
                    }
                }
                if let Some(i) = best {
                    let mut prof = self.profile.write().unwrap();
                    if prof.axes.len() <= i {
                        prof.axes.resize(6, Default::default());
                    }
                    prof.axes[i].enabled = true;
                    prof.axes[i].dest = dest;
                    self.status =
                        format!("assigned {} → {}", DOF_NAMES[i], dest.short_label());
                    self.assigning = None;
                } else {
                    self.status = format!(
                        "move cap for {}… ({} @ {:.2})",
                        dest.short_label(),
                        DOF_NAMES[live.0],
                        live.1
                    );
                }
            }
        }

        egui::Panel::right("gamepad")
            .exact_size(210.0)
            .show(ui, |ui| {
                gamepad_view(ui, &out, &pressed_set);
                ui.separator();
                ui.label(egui::RichText::new("Quick assign").strong());
                if let Some((dest, _, _)) = self.assigning {
                    ui.label(format!("move cap for {}", dest.short_label()));
                    if ui.button("Cancel").clicked() {
                        self.assigning = None;
                        self.status = "assign cancelled".into();
                    }
                } else {
                    if self.pipeline.is_none() {
                        ui.label("Start first, then click\nan output axis");
                    } else {
                        ui.label("click output axis,\nthen move the cap");
                    }
                    ui.add_enabled_ui(self.pipeline.is_some(), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for ax in OutAxis::ALL {
                                if ui.button(ax.short_label()).clicked() {
                                    self.assigning =
                                        Some((ax, state.axes, std::time::Instant::now()));
                                }
                            }
                        });
                    });
                }
                ui.separator();
                ui.label(format!("src keys: {:?}", state.buttons));
            });

        egui::Panel::bottom("profiles").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Profile:");
                egui::ComboBox::from_id_salt("profile")
                    .selected_text(
                        self.profile_sel.clone().unwrap_or_else(|| "(unsaved)".into()),
                    )
                    .show_ui(ui, |ui| {
                        for n in &self.profile_names {
                            ui.selectable_value(&mut self.profile_sel, Some(n.clone()), n);
                        }
                    });
                if ui.button("Load").clicked() {
                    if let Some(n) = &self.profile_sel {
                        match profile::load(n) {
                            Ok(p) => {
                                *self.profile.write().unwrap() = p;
                                self.status = format!("loaded {n}");
                            }
                            Err(e) => self.status = format!("load failed: {e:#}"),
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("theme")
                        .width(72.0)
                        .selected_text(match self.theme {
                            egui::ThemePreference::Dark => "Dark",
                            egui::ThemePreference::Light => "Light",
                            egui::ThemePreference::System => "Auto",
                        })
                        .show_ui(ui, |ui| {
                            for (t, l) in [
                                (egui::ThemePreference::System, "Auto"),
                                (egui::ThemePreference::Dark, "Dark"),
                                (egui::ThemePreference::Light, "Light"),
                            ] {
                                ui.selectable_value(&mut self.theme, t, l);
                            }
                        });
                    ui.label("theme:");
                });
                ui.ctx().options_mut(|o| {
                    o.theme_preference = match self.theme {
                        egui::ThemePreference::System => match self.system_dark {
                            // winit can't see the portal color-scheme on
                            // Wayland, so resolve "no preference" to Light
                            // (GNOME's visual default).
                            Some(true) => egui::ThemePreference::Dark,
                            Some(false) => egui::ThemePreference::Light,
                            None => egui::ThemePreference::Dark,
                        },
                        t => t,
                    };
                });
                ui.separator();
                ui.label("Save as:");
                ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(140.0));
                if ui.button("Save").clicked() && !self.save_name.trim().is_empty() {
                    let mut p = self.profile.write().unwrap();
                    p.name = self.save_name.trim().to_string();
                    match profile::save(&p) {
                        Ok(path) => {
                            self.profile_names = profile::list_profiles();
                            self.profile_sel = Some(p.name.clone());
                            self.status = format!("saved {}", path.display());
                        }
                        Err(e) => self.status = format!("save failed: {e:#}"),
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                ui.heading("Axes");
                // conflict detection: enabled axes sharing a dest
                let conflicts: Vec<usize> = {
                    let prof = self.profile.read().unwrap();
                    (0..6)
                        .map(|d| {
                            let ax = OutAxis::ALL[d];
                            prof.axes
                                .iter()
                                .take(6)
                                .filter(|m| {
                                    m.enabled && (m.dest == ax || m.dest2 == Some(ax))
                                })
                                .count()
                        })
                        .collect()
                };
                for i in 0..6 {
                    let mut prof = self.profile.write().unwrap();
                    if prof.axes.len() <= i {
                        prof.axes.resize(6, Default::default());
                    }
                    let map = &mut prof.axes[i];
                    // Fixed-size card: Frame::group fills its parent's max
                    // rect, and a child ui inherits the parent's layout, so
                    // we allocate a bounded rect and force a top-down layout.
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(440.0, 148.0),
                        egui::Sense::hover(),
                    );
                    let mut card = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    card.spacing_mut().slider_width = 110.0;
                    egui::Frame::group(card.style()).show(&mut card, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(DOF_NAMES[i]).strong());
                            ui.label("in");
                            axis_bar(ui, state.axes[i], 70.0);
                            ui.label("→");
                            let ov = if map.enabled {
                                out[map.dest.code() as usize]
                            } else {
                                0.0
                            };
                            axis_bar(ui, ov, 70.0);
                            ui.label("out");
                            let shared = map.enabled
                                && (conflicts[map.dest.code() as usize] > 1
                                    || map.dest2.is_some_and(|d2| {
                                        conflicts[d2.code() as usize] > 1
                                    }));
                            if shared {
                                ui.label(
                                    egui::RichText::new("⚠ dest shared")
                                        .color(egui::Color32::YELLOW),
                                );
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut map.enabled, "on");
                            ui.label("→");
                            egui::ComboBox::from_id_salt(("dest", i))
                                .width(52.0)
                                .selected_text(map.dest.short_label())
                                .show_ui(ui, |ui| {
                                    for ax in OutAxis::ALL {
                                        ui.selectable_value(&mut map.dest, ax, ax.label());
                                    }
                                });
                            ui.label("+");
                            egui::ComboBox::from_id_salt(("dest2", i))
                                .width(52.0)
                                .selected_text(
                                    map.dest2
                                        .map(|d| d.short_label())
                                        .unwrap_or("none"),
                                )
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(map.dest2.is_none(), "none")
                                        .clicked()
                                    {
                                        map.dest2 = None;
                                    }
                                    for ax in OutAxis::ALL {
                                        ui.selectable_value(
                                            &mut map.dest2,
                                            Some(ax),
                                            ax.label(),
                                        );
                                    }
                                });
                            ui.checkbox(&mut map.invert, "invert");
                            ui.label("mode:");
                            egui::ComboBox::from_id_salt(("mode", i))
                                .selected_text(map.mode.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut map.mode,
                                        AxisMode::Direct,
                                        AxisMode::Direct.label(),
                                    );
                                    ui.selectable_value(
                                        &mut map.mode,
                                        AxisMode::Integral,
                                        AxisMode::Integral.label(),
                                    );
                                });
                        });
                        ui.horizontal(|ui| {
                            curve_plot(ui, map, state.axes[i]);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label("deadzone");
                                    ui.add(
                                        egui::Slider::new(&mut map.deadzone, 0.0..=0.9)
                                            .fixed_decimals(2),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("expo");
                                    ui.add(
                                        egui::Slider::new(&mut map.expo, 0.0..=0.95)
                                            .fixed_decimals(2),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label("gain");
                                    ui.add(
                                        egui::Slider::new(&mut map.gain, 0.0..=4.0)
                                            .fixed_decimals(2),
                                    );
                                    if map.mode == AxisMode::Integral {
                                        ui.label("rate/s");
                                        ui.add(
                                            egui::Slider::new(&mut map.rate, 0.1..=5.0)
                                                .fixed_decimals(2),
                                        );
                                    }
                                });
                            });
                        });
                    });
                }

                ui.add_space(8.0);
                ui.heading("Buttons");
                {
                    let mut prof = self.profile.write().unwrap();
                    let mut remove = None;
                    for (i, b) in prof.buttons.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label("src:");
                            let capturing = matches!(self.capturing, Some((row, _)) if row == i);
                            let label = if capturing {
                                "press device button...".to_string()
                            } else {
                                format!("0x{:03X}", b.src)
                            };
                            if ui.button(label).clicked() && !capturing {
                                self.capturing = Some((i, state.last_key));
                            }
                            if capturing {
                                if let Some((row, baseline)) = self.capturing {
                                    if row == i
                                        && state.last_key.is_some()
                                        && state.last_key != baseline
                                    {
                                        b.src = state.last_key.unwrap();
                                        self.capturing = None;
                                    }
                                }
                            }
                            ui.label("→");
                            egui::ComboBox::from_id_salt(("bdest", i))
                                .selected_text(b.dest.label())
                                .show_ui(ui, |ui| {
                                    for k in DestKey::ALL {
                                        ui.selectable_value(&mut b.dest, k, k.label());
                                    }
                                });
                            if ui.button("x").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        prof.buttons.remove(i);
                    }
                    if ui.button("+ add button mapping").clicked() {
                        prof.buttons.push(ButtonMap::default());
                    }
                }
            });
        });
    }
}
