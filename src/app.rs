use std::sync::Arc;

use eframe::egui::{self, Color32, RichText};
use tokio::runtime::Runtime;

use drm1000_gui::protocol::{Mode, Request, opcode};
use drm1000_gui::serial::{SerialController, SerialSnapshot};

pub struct Drm1000App {
    _runtime: Arc<Runtime>,
    serial: SerialController,
    snapshot: SerialSnapshot,
    selected_port: String,
    frequency: String,
    volume: u8,
    register_address: String,
    register_value: String,
    register_write_armed: bool,
    show_diagnostics: bool,
}

impl Drm1000App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        configure_style(&cc.egui_ctx);
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?,
        );
        let serial = SerialController::spawn(&runtime, cc.egui_ctx.clone());
        let snapshot = serial.snapshot();
        Ok(Self {
            _runtime: runtime,
            serial,
            snapshot,
            selected_port: String::new(),
            frequency: "1000000".to_owned(),
            volume: 50,
            register_address: "00".to_owned(),
            register_value: "00".to_owned(),
            register_write_armed: false,
            show_diagnostics: false,
        })
    }

    fn send(&self, request: Request) {
        self.serial.send(request);
    }

    fn set_mode(&self, mode: Mode) {
        if let Some(value) = mode.set_value() {
            self.send(Request::byte(opcode::SET_MODE, value));
        }
    }

    fn tune(&mut self) {
        match parse_frequency(&self.frequency) {
            Ok(hz) => {
                self.frequency = hz.to_string();
                self.send(Request::word(opcode::TUNE_TO_FREQUENCY, hz));
                self.send(Request::new(opcode::GET_TUNED_FREQ));
            }
            Err(error) => self.snapshot.last_error = Some(error),
        }
    }

    fn parse_hex_byte(value: &str, field: &str) -> Result<u8, String> {
        u8::from_str_radix(value.trim().trim_start_matches("0x"), 16)
            .map_err(|_| format!("{field} must be a hexadecimal byte from 00 to FF"))
    }
}

impl eframe::App for Drm1000App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(snapshot) = self.serial.try_snapshot() {
            if let Some(frequency) = snapshot.frequency_hz {
                self.frequency = frequency.to_string();
            }
            if let Some(volume) = snapshot.volume {
                self.volume = volume;
            }
            if self.selected_port.is_empty() {
                if let Some(port) = snapshot.ports.iter().find(|port| port.likely_device) {
                    self.selected_port = port.port_name.clone();
                } else if snapshot.ports.len() == 1 {
                    self.selected_port = snapshot.ports[0].port_name.clone();
                }
            }
            self.snapshot = snapshot;
        }

        egui::TopBottomPanel::top("connection").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("DRM1000").color(Color32::from_rgb(237, 242, 244)));
                ui.label(
                    RichText::new("Broadcast receiver control")
                        .color(Color32::from_rgb(157, 174, 183)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let connected = self.snapshot.connected_port.is_some();
                    if ui
                        .button(if connected { "Disconnect" } else { "Connect" })
                        .clicked()
                    {
                        if connected {
                            self.serial.disconnect();
                        } else if !self.selected_port.is_empty() {
                            self.serial.connect(self.selected_port.clone());
                        }
                    }
                    egui::ComboBox::from_id_salt("port")
                        .selected_text(if self.selected_port.is_empty() {
                            "Select serial port"
                        } else {
                            &self.selected_port
                        })
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for port in &self.snapshot.ports {
                                ui.selectable_value(
                                    &mut self.selected_port,
                                    port.port_name.clone(),
                                    format!("{} - {}", port.port_name, port.summary),
                                );
                            }
                        });
                    if ui
                        .button("Refresh")
                        .on_hover_text("Refresh serial ports")
                        .clicked()
                    {
                        self.serial.refresh_ports();
                    }
                });
            });
            ui.add_space(8.0);
        });

        egui::SidePanel::right("status")
            .resizable(false)
            .default_width(285.0)
            .show(ctx, |ui| {
                ui.heading("Signal and status");
                ui.separator();
                status_row(ui, "Connection", &self.snapshot.connection_status);
                status_row(
                    ui,
                    "Firmware",
                    self.snapshot.firmware_version.as_deref().unwrap_or("-"),
                );
                status_row(
                    ui,
                    "Frequency",
                    &self
                        .snapshot
                        .frequency_hz
                        .map(format_frequency)
                        .unwrap_or_else(|| "-".to_owned()),
                );
                status_row(
                    ui,
                    "Mode",
                    &self
                        .snapshot
                        .mode
                        .map(|mode| mode.to_string())
                        .unwrap_or_else(|| "-".to_owned()),
                );
                if let Some(status) = &self.snapshot.status {
                    ui.add_space(10.0);
                    let rssi = status.rssi_dbm.clamp(-128.0, 0.0);
                    ui.label(format!("RSSI  {:.1} dBm", status.rssi_dbm));
                    ui.add(
                        egui::ProgressBar::new((rssi + 128.0) / 128.0)
                            .fill(Color32::from_rgb(40, 177, 129)),
                    );
                    status_row(ui, "Signal", yes_no(status.signal_detected));
                    status_row(ui, "DRM sync", yes_no(status.frame_synced));
                    status_row(ui, "FAC MER", &format!("{:.1} dB", status.fac_mer_db));
                    status_row(ui, "SDC MER", &format!("{:.1} dB", status.sdc_mer_db));
                    status_row(ui, "MSC MER", &format!("{:.1} dB", status.msc_mer_db));
                    status_row(
                        ui,
                        "Sample offset",
                        &format!("{:.2} ppm", status.sample_rate_offset_ppm),
                    );
                    status_row(
                        ui,
                        "Carrier offset",
                        &format!("{:.3}", status.frequency_offset),
                    );
                    if !status.services.is_empty() {
                        ui.add_space(10.0);
                        ui.label(RichText::new("Services").strong());
                        for service in &status.services {
                            let title = if service.label.is_empty() {
                                format!("Service {:06X}", service.id)
                            } else {
                                service.label.clone()
                            };
                            if ui.selectable_label(false, title).clicked() {
                                self.send(Request::word(opcode::TUNE_TO_STATION_ID, service.id));
                            }
                        }
                    }
                }
                if let Some(message) = &self.snapshot.text_message {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Broadcast text").strong());
                    ui.label(message);
                }
                if let Some(error) = &self.snapshot.last_error {
                    ui.add_space(12.0);
                    ui.colored_label(Color32::from_rgb(242, 118, 109), error);
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);
            ui.heading("Tuning");
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [250.0, 34.0],
                    egui::TextEdit::singleline(&mut self.frequency).hint_text("Frequency in Hz"),
                );
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    self.tune();
                }
                if ui
                    .add_sized([70.0, 34.0], egui::Button::new("Tune"))
                    .clicked()
                {
                    self.tune();
                }
            });
            ui.label(
                RichText::new("Accepts Hz, kHz, or MHz (for example 15.2 MHz)")
                    .small()
                    .color(Color32::from_rgb(145, 158, 164)),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                for (mode, label) in [
                    (Mode::Drm, "DRM"),
                    (Mode::AmWide, "AM wide"),
                    (Mode::AmNarrow, "AM narrow"),
                    (Mode::Fm, "FM"),
                ] {
                    let active = self.snapshot.mode == Some(mode);
                    if ui.selectable_label(active, label).clicked() {
                        self.set_mode(mode);
                    }
                }
            });
            ui.add_space(18.0);
            ui.heading("Station control");
            ui.horizontal(|ui| {
                if ui.button("Previous").clicked() {
                    self.send(Request::new(opcode::STATION_DOWN));
                }
                if ui.button("Next").clicked() {
                    self.send(Request::new(opcode::STATION_UP));
                }
                if ui.button("Start full scan").clicked() {
                    self.send(Request::word(opcode::START_SCAN, 0));
                    self.send(Request::new(opcode::GET_SCANNER_PROGRESS));
                }
                if ui.button("Load results").clicked() {
                    self.send(Request::new(opcode::GET_SCANNED_STATIONS));
                }
            });
            if let Some(progress) = self.snapshot.scanner_progress {
                ui.add(
                    egui::ProgressBar::new(f32::from(progress) / 100.0)
                        .text(format!("Scan {progress}%")),
                );
            }
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for station in &self.snapshot.stations {
                        let label = station
                            .services
                            .first()
                            .map(|service| service.label.as_str())
                            .filter(|label| !label.is_empty())
                            .unwrap_or("Unnamed station");
                        ui.horizontal(|ui| {
                            if ui.selectable_label(false, label).clicked() {
                                if let Some(service) = station.services.first() {
                                    self.send(Request::word(
                                        opcode::TUNE_TO_STATION_ID,
                                        service.id,
                                    ));
                                } else {
                                    self.send(Request::word(
                                        opcode::TUNE_TO_FREQUENCY,
                                        station.frequency_hz,
                                    ));
                                }
                            }
                            ui.label(
                                RichText::new(format!(
                                    "{}  {}",
                                    station.mode,
                                    format_frequency(station.frequency_hz)
                                ))
                                .color(Color32::from_rgb(145, 158, 164)),
                            );
                        });
                    }
                });
            ui.add_space(14.0);
            ui.heading("Audio");
            ui.horizontal(|ui| {
                ui.label("Volume");
                if ui
                    .add(egui::Slider::new(&mut self.volume, 0..=100).show_value(true))
                    .changed()
                {
                    self.send(Request::byte(opcode::SET_VOLUME, self.volume));
                }
                if ui
                    .button("Test tone")
                    .on_hover_text("Toggle the AM-mode audio test tone")
                    .clicked()
                {
                    self.send(Request::byte(opcode::AUDIO_TEST_TONE, 1));
                }
                if ui.button("Stop tone").clicked() {
                    self.send(Request::byte(opcode::AUDIO_TEST_TONE, 0));
                }
            });
            ui.add_space(16.0);
            ui.collapsing("CMX918 register access", |ui| {
                ui.colored_label(
                    Color32::from_rgb(238, 186, 72),
                    "Register writes take effect immediately and may disrupt reception.",
                );
                ui.horizontal(|ui| {
                    ui.label("Address 0x");
                    ui.add_sized(
                        [48.0, 24.0],
                        egui::TextEdit::singleline(&mut self.register_address),
                    );
                    if ui.button("Read").clicked() {
                        match Self::parse_hex_byte(&self.register_address, "address") {
                            Ok(address) => {
                                self.send(Request::byte(opcode::CMX918_REG_READ, address))
                            }
                            Err(error) => self.snapshot.last_error = Some(error),
                        }
                    }
                    ui.label("Value 0x");
                    ui.add_sized(
                        [48.0, 24.0],
                        egui::TextEdit::singleline(&mut self.register_value),
                    );
                });
                ui.checkbox(&mut self.register_write_armed, "Enable register writes");
                ui.add_enabled_ui(self.register_write_armed, |ui| {
                    if ui.button("Write register").clicked() {
                        match (
                            Self::parse_hex_byte(&self.register_address, "address"),
                            Self::parse_hex_byte(&self.register_value, "value"),
                        ) {
                            (Ok(address), Ok(value)) => {
                                self.send(Request::bytes(
                                    opcode::CMX918_REG_WRITE,
                                    &[address, value],
                                ));
                                self.register_write_armed = false;
                            }
                            (Err(error), _) | (_, Err(error)) => {
                                self.snapshot.last_error = Some(error)
                            }
                        }
                    }
                });
                if let Some((address, value)) = self.snapshot.last_register {
                    ui.label(format!("Last read: [0x{address:02X}] = 0x{value:02X}"));
                }
            });
            ui.add_space(8.0);
            ui.checkbox(&mut self.show_diagnostics, "Show serial diagnostics");
            if self.show_diagnostics {
                egui::ScrollArea::vertical()
                    .max_height(130.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.snapshot.log_lines {
                            ui.monospace(line);
                        }
                    });
            }
        });
    }
}

fn configure_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(24, 29, 32);
    visuals.window_fill = Color32::from_rgb(30, 36, 39);
    visuals.selection.bg_fill = Color32::from_rgb(31, 126, 161);
    visuals.widgets.active.bg_fill = Color32::from_rgb(34, 145, 178);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 78, 88);
    visuals.window_corner_radius = 6.0.into();
    ctx.set_visuals(visuals);
}

fn status_row(ui: &mut egui::Ui, name: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).color(Color32::from_rgb(145, 158, 164)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(value);
        });
    });
}

fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

fn format_frequency(hz: u32) -> String {
    if hz >= 1_000_000 {
        format!("{:.6} MHz", hz as f64 / 1_000_000.0)
    } else if hz >= 1_000 {
        format!("{:.3} kHz", hz as f64 / 1_000.0)
    } else {
        format!("{hz} Hz")
    }
}

fn parse_frequency(value: &str) -> Result<u32, String> {
    let normalized = value.trim().to_ascii_lowercase().replace(' ', "");
    let (number, multiplier) = if let Some(number) = normalized.strip_suffix("mhz") {
        (number, 1_000_000.0)
    } else if let Some(number) = normalized.strip_suffix("khz") {
        (number, 1_000.0)
    } else if let Some(number) = normalized.strip_suffix("hz") {
        (number, 1.0)
    } else {
        (normalized.as_str(), 1.0)
    };
    let hz = number
        .parse::<f64>()
        .map_err(|_| "invalid frequency".to_owned())?
        * multiplier;
    if !hz.is_finite() || !(1.0..=u32::MAX as f64).contains(&hz) {
        return Err("frequency is outside the supported numeric range".to_owned());
    }
    Ok(hz.round() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_human_frequencies() {
        assert_eq!(parse_frequency("15.2 MHz").unwrap(), 15_200_000);
        assert_eq!(parse_frequency("999 kHz").unwrap(), 999_000);
        assert_eq!(parse_frequency("1000000").unwrap(), 1_000_000);
    }
}
