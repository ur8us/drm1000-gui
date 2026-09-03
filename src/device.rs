use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{ClearBuffer, SerialPort, SerialPortBuilderExt, SerialStream};

use crate::protocol::{Frame, FrameDecoder, Request, opcode};

pub struct DeviceConnection {
    stream: SerialStream,
    decoder: FrameDecoder,
    read_buffer: [u8; 4096],
}

impl DeviceConnection {
    pub async fn open(port_name: &str, baud_rate: u32) -> Result<Self> {
        let mut stream = tokio_serial::new(port_name, baud_rate)
            .data_bits(tokio_serial::DataBits::Eight)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .flow_control(tokio_serial::FlowControl::None)
            .open_native_async()
            .with_context(|| format!("could not open serial port {port_name}"))?;
        #[cfg(unix)]
        stream
            .set_exclusive(false)
            .context("could not disable exclusive serial access")?;
        let _ = stream.clear(ClearBuffer::All);
        let _ = stream.write_data_terminal_ready(true);
        let _ = stream.write_request_to_send(true);
        Ok(Self {
            stream,
            decoder: FrameDecoder::default(),
            read_buffer: [0; 4096],
        })
    }

    pub async fn send(&mut self, request: &Request) -> Result<()> {
        self.stream
            .write_all(&request.encode())
            .await
            .context("serial write failed")?;
        self.stream.flush().await.context("serial flush failed")?;
        Ok(())
    }

    pub async fn next_frame(&mut self) -> Result<Frame> {
        loop {
            if let Some(frame) = self.decoder.next_frame() {
                return Ok(frame);
            }
            let count = self
                .stream
                .read(&mut self.read_buffer)
                .await
                .context("serial read failed")?;
            if count == 0 {
                bail!("serial port closed");
            }
            self.decoder.push(&self.read_buffer[..count]);
        }
    }

    pub async fn transact(&mut self, request: &Request, timeout: Duration) -> Result<Frame> {
        self.send(request).await?;
        let expected = request.opcode;
        tokio::time::timeout(timeout, async {
            loop {
                let frame = self.next_frame().await?;
                if frame.opcode == expected {
                    return Ok(frame);
                }
            }
        })
        .await
        .with_context(|| format!("timeout waiting for response to opcode 0x{expected:02X}"))?
    }

    pub async fn initialise(&mut self, timeout: Duration) -> Result<()> {
        let frame = self
            .transact(&Request::new(opcode::UART_INIT), timeout)
            .await?;
        frame.require_ok().map_err(anyhow::Error::msg)
    }
}
