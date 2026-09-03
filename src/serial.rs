use std::collections::VecDeque;
use std::time::Duration;

use eframe::egui;
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, watch};
use tokio_serial::{SerialPortType, available_ports};

use crate::device::DeviceConnection;
use crate::protocol::{self, Frame, Mode, ReceiverStatus, Request, ScannedStation, opcode};

const MAX_LOG_LINES: usize = 160;

#[derive(Clone, Debug, Default)]
pub struct PortSummary {
    pub port_name: String,
    pub summary: String,
    pub likely_device: bool,
}

#[derive(Clone, Debug)]
pub struct SerialSnapshot {
    pub connection_status: String,
    pub ports: Vec<PortSummary>,
    pub connected_port: Option<String>,
    pub firmware_version: Option<String>,
    pub frequency_hz: Option<u32>,
    pub mode: Option<Mode>,
    pub volume: Option<u8>,
    pub scanner_progress: Option<u8>,
    pub stations: Vec<ScannedStation>,
    pub status: Option<ReceiverStatus>,
    pub text_message: Option<String>,
    pub last_register: Option<(u8, u8)>,
    pub last_error: Option<String>,
    pub log_lines: Vec<String>,
    pub generation: u64,
}

impl Default for SerialSnapshot {
    fn default() -> Self {
        Self {
            connection_status: "idle".to_owned(),
            ports: Vec::new(),
            connected_port: None,
            firmware_version: None,
            frequency_hz: None,
            mode: None,
            volume: None,
            scanner_progress: None,
            stations: Vec::new(),
            status: None,
            text_message: None,
            last_register: None,
            last_error: None,
            log_lines: vec!["serial controller started".to_owned()],
            generation: 0,
        }
    }
}

pub struct SerialController {
    command_tx: mpsc::UnboundedSender<ControllerCommand>,
    snapshot_rx: watch::Receiver<SerialSnapshot>,
}

impl SerialController {
    pub fn spawn(runtime: &Runtime, repaint_ctx: egui::Context) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (snapshot_tx, snapshot_rx) = watch::channel(SerialSnapshot::default());
        runtime.spawn(controller_task(command_rx, snapshot_tx, repaint_ctx));
        Self {
            command_tx,
            snapshot_rx,
        }
    }

    pub fn snapshot(&self) -> SerialSnapshot {
        self.snapshot_rx.borrow().clone()
    }
    pub fn try_snapshot(&mut self) -> Option<SerialSnapshot> {
        self.snapshot_rx
            .has_changed()
            .ok()
            .filter(|changed| *changed)
            .map(|_| self.snapshot_rx.borrow_and_update().clone())
    }
    pub fn refresh_ports(&self) {
        let _ = self.command_tx.send(ControllerCommand::RefreshPorts);
    }
    pub fn connect(&self, port: String) {
        let _ = self.command_tx.send(ControllerCommand::Connect(port));
    }
    pub fn disconnect(&self) {
        let _ = self.command_tx.send(ControllerCommand::Disconnect);
    }
    pub fn send(&self, request: Request) {
        let _ = self.command_tx.send(ControllerCommand::Send(request));
    }
}

enum ControllerCommand {
    RefreshPorts,
    Connect(String),
    Disconnect,
    Send(Request),
}

async fn controller_task(
    mut command_rx: mpsc::UnboundedReceiver<ControllerCommand>,
    snapshot_tx: watch::Sender<SerialSnapshot>,
    repaint: egui::Context,
) {
    let mut snapshot = SerialSnapshot::default();
    refresh_ports(&mut snapshot);
    publish(&snapshot_tx, &repaint, &mut snapshot);
    let mut connection: Option<DeviceConnection> = None;
    let mut scan_poll = tokio::time::interval(Duration::from_millis(500));

    loop {
        if connection.is_none() {
            let Some(command) = command_rx.recv().await else {
                break;
            };
            handle_command(command, &mut connection, &mut snapshot).await;
            publish(&snapshot_tx, &repaint, &mut snapshot);
            continue;
        }

        tokio::select! {
            Some(command) = command_rx.recv() => {
                handle_command(command, &mut connection, &mut snapshot).await;
                publish(&snapshot_tx, &repaint, &mut snapshot);
            }
            result = connection.as_mut().unwrap().next_frame() => {
                match result {
                    Ok(frame) => process_frame(frame, &mut snapshot),
                    Err(error) => {
                        snapshot.connection_status = "error".to_owned();
                        snapshot.connected_port = None;
                        snapshot.last_error = Some(error.to_string());
                        connection = None;
                    }
                }
                publish(&snapshot_tx, &repaint, &mut snapshot);
            }
            _ = scan_poll.tick(), if snapshot.scanner_progress.is_some_and(|progress| progress < 100) => {
                if let Some(device) = &mut connection {
                    let _ = device.send(&Request::new(opcode::GET_SCANNER_PROGRESS)).await;
                }
            }
        }
    }
}

async fn handle_command(
    command: ControllerCommand,
    connection: &mut Option<DeviceConnection>,
    snapshot: &mut SerialSnapshot,
) {
    match command {
        ControllerCommand::RefreshPorts => refresh_ports(snapshot),
        ControllerCommand::Disconnect => {
            *connection = None;
            snapshot.connected_port = None;
            snapshot.connection_status = "disconnected".to_owned();
            push_log(snapshot, "disconnected".to_owned());
        }
        ControllerCommand::Connect(port) => {
            snapshot.connection_status = "connecting".to_owned();
            match DeviceConnection::open(&port, protocol::BAUD_RATE).await {
                Ok(mut device) => {
                    if let Err(error) = device.initialise(Duration::from_secs(3)).await {
                        snapshot.last_error = Some(error.to_string());
                        snapshot.connection_status = "error".to_owned();
                        return;
                    }
                    for request in initial_requests() {
                        let _ = device.send(&request).await;
                    }
                    snapshot.connected_port = Some(port.clone());
                    snapshot.connection_status = "connected".to_owned();
                    snapshot.last_error = None;
                    push_log(
                        snapshot,
                        format!("connected to {port} at {} baud", protocol::BAUD_RATE),
                    );
                    *connection = Some(device);
                }
                Err(error) => {
                    snapshot.connection_status = "error".to_owned();
                    snapshot.last_error = Some(error.to_string());
                }
            }
        }
        ControllerCommand::Send(request) => {
            if let Some(device) = connection {
                let opcode = request.opcode;
                if opcode == opcode::CMX918_REG_READ
                    && let Some(address) = request.parameters.first()
                {
                    snapshot.last_register = Some((*address, 0));
                }
                if opcode == opcode::START_SCAN {
                    snapshot.scanner_progress = Some(0);
                    snapshot.stations.clear();
                }
                match device.send(&request).await {
                    Ok(()) => push_log(snapshot, format!("> opcode 0x{opcode:02X}")),
                    Err(error) => snapshot.last_error = Some(error.to_string()),
                }
            } else {
                snapshot.last_error = Some("not connected".to_owned());
            }
        }
    }
}

fn initial_requests() -> Vec<Request> {
    vec![
        Request::new(opcode::GET_VERSION),
        Request::new(opcode::GET_TUNED_FREQ),
        Request::new(opcode::GET_DEMOD_MODE),
        Request::new(opcode::GET_VOLUME),
        Request::byte(opcode::TEXT_MSG_OUT, 1),
        Request::byte(opcode::STATUS_OUTPUT, 1),
    ]
}

fn process_frame(frame: Frame, snapshot: &mut SerialSnapshot) {
    push_log(
        snapshot,
        format!(
            "< opcode 0x{:02X}, {} data bytes",
            frame.opcode,
            frame.payload.len()
        ),
    );
    if frame.error != 0 {
        snapshot.last_error = Some(format!(
            "0x{:02X}: {}",
            frame.error,
            protocol::error_description(frame.error)
        ));
        return;
    }
    let result: Result<(), String> = (|| {
        match frame.opcode {
            opcode::GET_VERSION => {
                snapshot.firmware_version = Some(
                    String::from_utf8_lossy(&frame.payload)
                        .trim_matches(char::from(0))
                        .to_owned(),
                )
            }
            opcode::GET_TUNED_FREQ => {
                snapshot.frequency_hz = Some(protocol::le_u32(&frame.payload, 0)?)
            }
            opcode::GET_DEMOD_MODE => {
                snapshot.mode = frame
                    .payload
                    .first()
                    .and_then(|value| Mode::from_status(*value))
            }
            opcode::GET_VOLUME => snapshot.volume = frame.payload.first().copied(),
            opcode::RSSI_GET => {
                snapshot
                    .status
                    .get_or_insert_with(Default::default)
                    .rssi_dbm = f32::from_bits(protocol::le_u32(&frame.payload, 0)?)
            }
            opcode::GET_SCANNER_PROGRESS => {
                snapshot.scanner_progress = frame.payload.first().copied()
            }
            opcode::GET_SCANNED_STATIONS => {
                snapshot.stations = protocol::parse_scanned_stations(&frame)?
            }
            opcode::GET_STATUS => {
                let status = protocol::parse_status(&frame)?;
                snapshot.mode = Some(status.mode);
                snapshot.status = Some(status);
            }
            opcode::TEXT_MSG_SERVICE => {
                snapshot.text_message = protocol::parse_text_message(&frame)?
            }
            opcode::CMX918_REG_READ => {
                if let Some(value) = frame.payload.first() {
                    let address = snapshot.last_register.map(|pair| pair.0).unwrap_or(0);
                    snapshot.last_register = Some((address, *value));
                }
            }
            _ => {}
        }
        Ok(())
    })();
    if let Err(error) = result {
        snapshot.last_error = Some(error);
    } else {
        snapshot.last_error = None;
    }
}

fn refresh_ports(snapshot: &mut SerialSnapshot) {
    snapshot.ports = available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|port| {
            let (summary, likely_device) = match port.port_type {
                SerialPortType::UsbPort(usb) => {
                    let text = format!(
                        "{} {:04X}:{:04X}",
                        usb.product.unwrap_or_else(|| "USB serial".to_owned()),
                        usb.vid,
                        usb.pid
                    );
                    let lower = text.to_ascii_lowercase();
                    (text, lower.contains("drm") || lower.contains("de9180"))
                }
                _ => ("serial port".to_owned(), false),
            };
            PortSummary {
                port_name: port.port_name,
                summary,
                likely_device,
            }
        })
        .collect();
}

fn push_log(snapshot: &mut SerialSnapshot, line: String) {
    let mut lines: VecDeque<_> = snapshot.log_lines.drain(..).collect();
    lines.push_back(line);
    while lines.len() > MAX_LOG_LINES {
        lines.pop_front();
    }
    snapshot.log_lines = lines.into();
}

fn publish(
    tx: &watch::Sender<SerialSnapshot>,
    repaint: &egui::Context,
    snapshot: &mut SerialSnapshot,
) {
    snapshot.generation += 1;
    let _ = tx.send(snapshot.clone());
    repaint.request_repaint_after(Duration::from_millis(50));
}
