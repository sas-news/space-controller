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

fn axis_bar(ui: &mut egui::Ui, value: f32, width: f32) {
    let height = 12.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    let bg = ui.visuals().extreme_bg_color;
    let accent = ui.visuals().selection.bg_fill;
    painter.rect_filled(rect, 3.0, bg);
    let mid = rect.center().x;
    let half = rect.width() / 2.0 - 2.0;
    let end = mid + value.clamp(-1.0, 1.0) * half;
    let bar = egui::Rect::from_x_y_ranges(
        mid.min(end)..=mid.max(end),
        rect.top() + 2.0..=rect.bottom() - 2.0,
    );
    painter.rect_filled(bar, 2.0, accent);
    painter.vline(
        mid,
        rect.y_range(),
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
    );
    painter.text(
        rect.right_center() - egui::vec2(4.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        format!("{:+.2}", value),
        egui::FontId::monospace(10.0),
        ui.visuals().text_color(),
    );
}

/// Small response-curve preview: y = shape(map, x) over -1..1.
fn curve_plot(ui: &mut egui::Ui, map: &crate::mapping::AxisMap, raw: f32) {
    let size = egui::vec2(96.0, 48.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    let bg = ui.visuals().extreme_bg_color;
    let accent = ui.visuals().selection.bg_fill;
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
        egui::Stroke::new(1.5, accent),
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
        ui.visuals().selection.bg_fill,
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
                    ui.visuals().selection.bg_fill,
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
                    ui.visuals().selection.bg_fill,
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
                            ui.visuals().selection.bg_fill
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
                            egui::Color32::BLACK
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
                ui.label(&self.status);
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
                ui.ctx()
                    .options_mut(|o| o.theme_preference = self.theme);
            });
        });

        let state = self.state.lock().unwrap().clone();
        let (out, out_pressed) = self.out_preview.lock().unwrap().clone();
        let pressed_set: HashSet<u16> = out_pressed.into_iter().collect();

        egui::Panel::right("gamepad")
            .exact_size(210.0)
            .show(ui, |ui| {
                gamepad_view(ui, &out, &pressed_set);
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
                            prof.axes
                                .iter()
                                .take(6)
                                .filter(|m| m.enabled && m.dest == OutAxis::ALL[d])
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
                            if map.enabled && conflicts[map.dest.code() as usize] > 1 {
                                ui.label(
                                    egui::RichText::new("⚠ dest shared")
                                        .color(egui::Color32::YELLOW),
                                );
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut map.enabled, "on");
                            ui.label("to:");
                            egui::ComboBox::from_id_salt(("dest", i))
                                .selected_text(map.dest.label())
                                .show_ui(ui, |ui| {
                                    for ax in OutAxis::ALL {
                                        ui.selectable_value(&mut map.dest, ax, ax.label());
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
