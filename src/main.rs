mod app;

use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use drm1000_gui::device::DeviceConnection;
use drm1000_gui::protocol::{self, Frame, Mode, PersistentConfig, Request, opcode};
use tokio_serial::{SerialPortType, available_ports};

#[derive(Parser)]
#[command(
    name = "drm1000-gui",
    version,
    about = "CML Micro DRM1000 receiver controller"
)]
struct Args {
    #[arg(
        long,
        global = true,
        help = "Serial port (for example /dev/ttyUSB0 or COM3)"
    )]
    port: Option<String>,
    #[arg(long, global = true, default_value_t = protocol::BAUD_RATE)]
    baud: u32,
    #[arg(long, global = true, default_value_t = 3000)]
    timeout_ms: u64,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Ports,
    Probe,
    Status,
    Tune {
        frequency: String,
    },
    Mode {
        mode: CliMode,
    },
    Scan {
        #[arg(default_value_t = 0)]
        band: u32,
    },
    ScanProgress,
    Stations,
    StationUp {
        #[arg(long)]
        raster: bool,
    },
    StationDown {
        #[arg(long)]
        raster: bool,
    },
    TuneStation {
        id: String,
    },
    Volume {
        value: Option<u8>,
    },
    Gain {
        mode: Option<GainMode>,
        #[arg(allow_hyphen_values = true)]
        value: Option<i8>,
        #[arg(long, help = "Confirm the persistent flash write")]
        confirm: bool,
        #[arg(
            long,
            help = "Use documented DRM1000/2.2 defaults when no stored config can be read"
        )]
        initialize_defaults: bool,
    },
    PresetRecall {
        slot: u8,
    },
    PresetStore {
        slot: u8,
    },
    BandUp,
    RegisterRead {
        address: String,
    },
    RegisterWrite {
        address: String,
        value: String,
        #[arg(long, help = "Confirm the potentially disruptive write")]
        confirm: bool,
    },
    Screen {
        state: OnOff,
    },
    TestTone {
        state: OnOff,
    },
    Standby {
        mode: StandbyMode,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum CliMode {
    Drm,
    AmWide,
    AmNarrow,
    Fm,
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
enum GainMode {
    Drm,
    Am,
    Fm,
}

impl GainMode {
    fn index(self) -> usize {
        match self {
            Self::Drm => 0,
            Self::Am => 1,
            Self::Fm => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Drm => "DRM",
            Self::Am => "AM",
            Self::Fm => "FM",
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum StandbyMode {
    DeepSleep,
    Restart,
    EmergencyWake,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.command.is_none() {
        return run_gui();
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_cli(args))
}

fn run_gui() -> Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([980.0, 760.0])
            .with_min_inner_size([820.0, 620.0]),
        #[cfg(target_os = "windows")]
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        &format!("DRM1000 GUI - {}", env!("BUILD_VERSION")),
        native_options,
        Box::new(|cc| {
            Ok(Box::new(app::Drm1000App::new(cc).map_err(
                |error| -> Box<dyn std::error::Error + Send + Sync> { error.into() },
            )?))
        }),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

async fn run_cli(args: Args) -> Result<()> {
    let command = args.command.unwrap();
    if matches!(command, Command::Ports) {
        return list_ports();
    }
    let port = args.port.context(
        "--port is required for device operations; use `drm1000-gui ports` to list ports",
    )?;
    let timeout = Duration::from_millis(args.timeout_ms);
    let mut device = DeviceConnection::open(&port, args.baud).await?;
    device
        .initialise(timeout)
        .await
        .context("DRM1000 UART initialisation failed")?;

    match command {
        Command::Ports => unreachable!(),
        Command::Probe => {
            let version = request(&mut device, Request::new(opcode::GET_VERSION), timeout).await?;
            let frequency =
                request(&mut device, Request::new(opcode::GET_TUNED_FREQ), timeout).await?;
            let mode = request(&mut device, Request::new(opcode::GET_DEMOD_MODE), timeout).await?;
            let volume = request(&mut device, Request::new(opcode::GET_VOLUME), timeout).await?;
            println!(
                "Firmware: {}",
                String::from_utf8_lossy(&version.payload).trim_matches(char::from(0))
            );
            println!(
                "Frequency: {} Hz",
                protocol::le_u32(&frequency.payload, 0).map_err(anyhow::Error::msg)?
            );
            println!(
                "Mode: {}",
                mode.payload
                    .first()
                    .and_then(|value| Mode::from_status(*value))
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned())
            );
            println!("Volume: {}", volume.payload.first().copied().unwrap_or(0));
        }
        Command::Status => {
            request(
                &mut device,
                Request::byte(opcode::STATUS_OUTPUT, 1),
                timeout,
            )
            .await?;
            let frame = wait_for(&mut device, opcode::GET_STATUS, timeout).await?;
            let status = protocol::parse_status(&frame).map_err(anyhow::Error::msg)?;
            println!(
                "Mode: {}\nSignal: {}\nDRM sync: {}\nRSSI: {:.2} dBm\nFAC/SDC/MSC MER: {:.2}/{:.2}/{:.2} dB\nServices: {}",
                status.mode,
                status.signal_detected,
                status.frame_synced,
                status.rssi_dbm,
                status.fac_mer_db,
                status.sdc_mer_db,
                status.msc_mer_db,
                status.services.len()
            );
            for service in status.services {
                println!("  {:06X}  {}", service.id, service.label);
            }
        }
        Command::Tune { frequency } => {
            let hz = parse_frequency(&frequency)?;
            request(
                &mut device,
                Request::word(opcode::TUNE_TO_FREQUENCY, hz),
                timeout,
            )
            .await?;
            println!("Tuned to {hz} Hz");
        }
        Command::Mode { mode } => {
            let mode = match mode {
                CliMode::Drm => Mode::Drm,
                CliMode::AmWide => Mode::AmWide,
                CliMode::AmNarrow => Mode::AmNarrow,
                CliMode::Fm => Mode::Fm,
            };
            request(
                &mut device,
                Request::byte(opcode::SET_MODE, mode.set_value().unwrap()),
                timeout,
            )
            .await?;
            println!("Mode set to {mode}");
        }
        Command::Scan { band } => {
            request(
                &mut device,
                Request::word(opcode::START_SCAN, band),
                timeout,
            )
            .await?;
            println!("Scan started for band selector {band}");
        }
        Command::ScanProgress => {
            let frame = request(
                &mut device,
                Request::new(opcode::GET_SCANNER_PROGRESS),
                timeout,
            )
            .await?;
            println!("{}%", one_byte(&frame)?);
        }
        Command::Stations => {
            let frame = request(
                &mut device,
                Request::new(opcode::GET_SCANNED_STATIONS),
                timeout,
            )
            .await?;
            for station in protocol::parse_scanned_stations(&frame).map_err(anyhow::Error::msg)? {
                let services = station
                    .services
                    .iter()
                    .map(|service| format!("{:06X}:{}", service.id, service.label))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "{} Hz\t{}\t{}",
                    station.frequency_hz, station.mode, services
                );
            }
        }
        Command::StationUp { raster } => {
            request(
                &mut device,
                Request::new(if raster {
                    opcode::STATION_UP_CHANNEL_RASTER
                } else {
                    opcode::STATION_UP
                }),
                timeout,
            )
            .await?;
        }
        Command::StationDown { raster } => {
            request(
                &mut device,
                Request::new(if raster {
                    opcode::STATION_DOWN_CHANNEL_RASTER
                } else {
                    opcode::STATION_DOWN
                }),
                timeout,
            )
            .await?;
        }
        Command::TuneStation { id } => {
            let id = parse_u32(&id)?;
            if id > 0x00ff_ffff {
                bail!("station ID must fit in 24 bits");
            }
            request(
                &mut device,
                Request::word(opcode::TUNE_TO_STATION_ID, id),
                timeout,
            )
            .await?;
        }
        Command::Volume { value } => {
            if let Some(value) = value {
                if value > 100 {
                    bail!("volume must be between 0 and 100");
                }
                request(
                    &mut device,
                    Request::byte(opcode::SET_VOLUME, value),
                    timeout,
                )
                .await?;
                println!("Volume set to {value}");
            } else {
                let frame = request(&mut device, Request::new(opcode::GET_VOLUME), timeout).await?;
                println!("{}", one_byte(&frame)?);
            }
        }
        Command::Gain {
            mode,
            value,
            confirm,
            initialize_defaults,
        } => {
            let frame = device
                .transact(
                    &Request::new(opcode::PERSISTENT_DEVICE_CONFIG_READ),
                    timeout,
                )
                .await?;
            let no_stored_config = frame.error == 11;
            let mut config = if no_stored_config {
                if mode.is_some() && !initialize_defaults {
                    bail!(
                        "the module has no readable persistent configuration; repeat with --initialize-defaults to write the documented DRM1000/2.2 defaults"
                    );
                }
                PersistentConfig::vendor_defaults()
            } else {
                PersistentConfig::from_frame(&frame).map_err(anyhow::Error::msg)?
            };
            match (mode, value) {
                (None, None) => {
                    let gains = config.audio_gains_db();
                    if no_stored_config {
                        println!("No stored configuration; showing built-in defaults.");
                    }
                    println!(
                        "DRM: {} dB\nAM: {} dB\nFM: {} dB",
                        gains[0], gains[1], gains[2]
                    );
                }
                (Some(mode), Some(value)) => {
                    if !confirm {
                        bail!("gain changes write persistent flash and require --confirm");
                    }
                    let mut gains = config.audio_gains_db();
                    gains[mode.index()] = value;
                    config
                        .set_audio_gains_db(gains)
                        .map_err(anyhow::Error::msg)?;
                    request(&mut device, config.write_request(), timeout).await?;
                    println!(
                        "{} audio gain saved as {} dB; restart the DRM1000 to apply it",
                        mode.label(),
                        value
                    );
                }
                _ => bail!("gain requires both MODE and VALUE, or neither when reading"),
            }
        }
        Command::PresetRecall { slot } | Command::PresetStore { slot } => {
            if slot > 3 {
                bail!("preset slot must be from 0 to 3");
            }
            let opcode = if matches!(command, Command::PresetRecall { .. }) {
                opcode::STATION_RECALL
            } else {
                opcode::STATION_STORE
            };
            request(&mut device, Request::byte(opcode, slot), timeout).await?;
        }
        Command::BandUp => {
            request(&mut device, Request::new(opcode::BAND_UP), timeout).await?;
        }
        Command::RegisterRead { address } => {
            let address = parse_byte(&address)?;
            let frame = request(
                &mut device,
                Request::byte(opcode::CMX918_REG_READ, address),
                timeout,
            )
            .await?;
            println!("[0x{address:02X}] = 0x{:02X}", one_byte(&frame)?);
        }
        Command::RegisterWrite {
            address,
            value,
            confirm,
        } => {
            if !confirm {
                bail!("register writes require --confirm");
            }
            let (address, value) = (parse_byte(&address)?, parse_byte(&value)?);
            request(
                &mut device,
                Request::bytes(opcode::CMX918_REG_WRITE, &[address, value]),
                timeout,
            )
            .await?;
            println!("Wrote 0x{value:02X} to register 0x{address:02X}");
        }
        Command::Screen { state } => {
            request(
                &mut device,
                Request::byte(opcode::SET_SCREEN_STATUS, on_off(state)),
                timeout,
            )
            .await?;
        }
        Command::TestTone { state } => {
            request(
                &mut device,
                Request::byte(opcode::AUDIO_TEST_TONE, on_off(state)),
                timeout,
            )
            .await?;
        }
        Command::Standby { mode } => {
            let value = match mode {
                StandbyMode::DeepSleep => 0,
                StandbyMode::Restart => 1,
                StandbyMode::EmergencyWake => 2,
            };
            request(&mut device, Request::byte(opcode::STANDBY, value), timeout).await?;
        }
    }
    Ok(())
}

async fn request(
    device: &mut DeviceConnection,
    request: Request,
    timeout: Duration,
) -> Result<Frame> {
    let frame = device.transact(&request, timeout).await?;
    frame.require_ok().map_err(anyhow::Error::msg)?;
    Ok(frame)
}

async fn wait_for(device: &mut DeviceConnection, wanted: u8, timeout: Duration) -> Result<Frame> {
    tokio::time::timeout(timeout, async {
        loop {
            let frame = device.next_frame().await?;
            if frame.opcode == wanted {
                return Ok(frame);
            }
        }
    })
    .await
    .context("timeout waiting for asynchronous device data")?
}

fn list_ports() -> Result<()> {
    for port in available_ports()? {
        let detail = match port.port_type {
            SerialPortType::UsbPort(usb) => format!(
                "USB {:04X}:{:04X} {}",
                usb.vid,
                usb.pid,
                usb.product.unwrap_or_default()
            ),
            _ => "serial".to_owned(),
        };
        println!("{}\t{}", port.port_name, detail);
    }
    Ok(())
}

fn parse_frequency(value: &str) -> Result<u32> {
    let value = value.trim().to_ascii_lowercase().replace(' ', "");
    let (number, multiplier) = if let Some(number) = value.strip_suffix("mhz") {
        (number, 1_000_000.0)
    } else if let Some(number) = value.strip_suffix("khz") {
        (number, 1_000.0)
    } else if let Some(number) = value.strip_suffix("hz") {
        (number, 1.0)
    } else {
        (value.as_str(), 1.0)
    };
    let hz = number.parse::<f64>().context("invalid frequency")? * multiplier;
    if !hz.is_finite() || !(1.0..=u32::MAX as f64).contains(&hz) {
        bail!("frequency is outside the supported numeric range");
    }
    Ok(hz.round() as u32)
}

fn parse_u32(value: &str) -> Result<u32> {
    if let Some(value) = value.strip_prefix("0x") {
        Ok(u32::from_str_radix(value, 16)?)
    } else {
        Ok(value.parse()?)
    }
}

fn parse_byte(value: &str) -> Result<u8> {
    let value = value.trim().trim_start_matches("0x");
    u8::from_str_radix(value, 16).context("expected a hexadecimal byte from 00 to FF")
}

fn one_byte(frame: &Frame) -> Result<u8> {
    frame
        .payload
        .first()
        .copied()
        .context("device returned no data byte")
}
fn on_off(value: OnOff) -> u8 {
    match value {
        OnOff::On => 1,
        OnOff::Off => 0,
    }
}
