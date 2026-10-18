use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use eframe::egui;

use crate::input::{self, Discovered};
use crate::mapping::{AxisMode, ButtonMap, DestKey, InputState, OutAxis, Profile, DOF_NAMES};
use crate::pipeline::{Pipeline, Source};
use crate::profile;

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([820.0, 720.0])
            .with_min_inner_size([640.0, 480.0]),
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
    out_preview: Arc<Mutex<[f32; 6]>>,
    profile: Arc<RwLock<Profile>>,
    profile_names: Vec<String>,
    profile_sel: Option<String>,
    save_name: String,
    devices: Vec<Discovered>,
    device_sel: usize,
    pipeline: Option<Pipeline>,
    status: String,
    capturing: Option<(usize, Option<u16>)>,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let profile_names = profile::list_profiles();
        let profile = profile_names
            .first()
            .and_then(|n| profile::load(n).ok())
            .unwrap_or_default();
        let mut app = Self {
            state: Arc::new(Mutex::new(InputState::default())),
            out_preview: Arc::new(Mutex::new([0.0; 6])),
            profile: Arc::new(RwLock::new(profile)),
            profile_sel: profile_names.first().cloned(),
            profile_names,
            save_name: String::new(),
            devices: input::find_devices(),
            device_sel: 0,
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
            self.state.clone(),
            self.profile.clone(),
            self.out_preview.clone(),
        ) {
            Ok(p) => {
                self.pipeline = Some(p);
                self.status = "running — virtual gamepad emitted".into();
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
    painter.vline(mid, rect.y_range(), egui::Stroke::new(1.0, ui.visuals().weak_text_color()));
    painter.text(
        rect.right_center() - egui::vec2(4.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        format!("{:+.2}", value),
        egui::FontId::monospace(10.0),
        ui.visuals().text_color(),
    );
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint_after(Duration::from_millis(33));

        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("space-controller");
                ui.separator();

                let running = self.pipeline.is_some();
                ui.add_enabled_ui(!running, |ui| {
                    let mut labels: Vec<String> = self
                        .devices
                        .iter()
                        .map(|d| d.name.clone())
                        .collect();
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
                });

                if running {
                    if ui.button("Stop").clicked() {
                        self.stop();
                    }
                } else if ui.button("Start").clicked() {
                    self.start();
                }
                ui.separator();
                ui.label(&self.status);
            });
        });

        egui::Panel::bottom("profiles").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Profile:");
                egui::ComboBox::from_id_salt("profile")
                    .selected_text(self.profile_sel.clone().unwrap_or_else(|| "(unsaved)".into()))
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
            let state = self.state.lock().unwrap().clone();
            let out = *self.out_preview.lock().unwrap();

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Axes");
                for i in 0..6 {
                    let mut prof = self.profile.write().unwrap();
                    if prof.axes.len() <= i {
                        prof.axes.resize(6, Default::default());
                    }
                    let map = &mut prof.axes[i];
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(
                                egui::RichText::new(format!("{i}  {}", DOF_NAMES[i])).strong(),
                            ));
                            ui.label("in");
                            axis_bar(ui, state.axes[i], 140.0);
                            ui.label("out");
                            let out_idx = if map.enabled {
                                map.dest.code() as usize
                            } else {
                                99
                            };
                            let ov = if out_idx < 6 { out[out_idx] } else { 0.0 };
                            axis_bar(ui, ov, 140.0);
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
                            ui.label("deadzone");
                            ui.add(egui::Slider::new(&mut map.deadzone, 0.0..=0.9));
                            ui.label("expo");
                            ui.add(egui::Slider::new(&mut map.expo, 0.0..=0.95));
                            ui.label("gain");
                            ui.add(egui::Slider::new(&mut map.gain, 0.0..=4.0));
                            if map.mode == AxisMode::Integral {
                                ui.label("rate/s");
                                ui.add(egui::Slider::new(&mut map.rate, 0.1..=5.0));
                            }
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

                ui.add_space(8.0);
                ui.label(format!("pressed src keys: {:?}", state.buttons));
            });
        });
    }
}
