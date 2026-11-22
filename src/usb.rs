use anyhow::{ensure, Context, Result};
use rusb::{Device, DeviceHandle, Direction, GlobalContext, TransferType};
use serde_json::{json, Value};
use std::{sync::mpsc, thread, time::{Duration, Instant}};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model { Mip001, Mip201, Mip301 }
impl Model {
    pub fn from_ids(vid: u16, pid: u16) -> Option<Self> {
        match (vid, pid) {
            (0x1c8a, 0x3a21) => Some(Self::Mip001),
            (0x36ec, 0x1001) => Some(Self::Mip201),
            (0x36ec, 0x1002) => Some(Self::Mip301),
            _ => None,
        }
    }
    fn out(self) -> u8 { if self == Self::Mip001 { 2 } else { 1 } }
    fn tokens(self) -> usize { if self == Self::Mip001 { 10 } else { 11 } }
}

pub fn list() -> Result<Value> {
    let mut rows = Vec::new();
    for device in rusb::devices()?.iter() {
        let d = device.device_descriptor()?;
        if let Some(model) = Model::from_ids(d.vendor_id(), d.product_id()) {
            let mut row = json!({"bus":device.bus_number(), "address":device.address(),
                "vid":format!("{:04x}", d.vendor_id()), "pid":format!("{:04x}", d.product_id()),
                "model":format!("{model:?}")});
            match device.open() {
                Ok(h) => {
                    row["product"] = json!(h.read_product_string_ascii(&d).ok());
                    row["serial"] = json!(h.read_serial_number_string_ascii(&d).ok());
                }
                Err(e) => row["access_error"] = json!(e.to_string()),
            }
            rows.push(row);
        }
    }
    Ok(json!(rows))
}

pub struct Session {
    handle: DeviceHandle<GlobalContext>,
    interface: u8,
    out: u8,
    input: Option<u8>,
    read_len: usize,
    pub model: Model,
}

impl Drop for Session {
    fn drop(&mut self) { let _ = self.handle.release_interface(self.interface); }
}

impl Session {
    pub fn open(bus: Option<u8>, address: Option<u8>, serial: Option<&str>, require_in: bool) -> Result<Self> {
        let mut candidates = Vec::new();
        for device in rusb::devices()?.iter() {
            let d = device.device_descriptor()?;
            if let Some(model) = Model::from_ids(d.vendor_id(), d.product_id()) {
                if bus.is_some_and(|b| b != device.bus_number()) || address.is_some_and(|a| a != device.address()) { continue; }
                if let Some(s) = serial {
                    let h = device.open().context("open USB to read serial; check udev permissions")?;
                    if h.read_serial_number_string_ascii(&d).ok().as_deref() != Some(s) { continue; }
                }
                candidates.push((device, model));
            }
        }
        ensure!(!candidates.is_empty(), "no matching Nemonic USB printer");
        ensure!(candidates.len() == 1, "multiple printers: select --bus/--address or --serial");
        let (device, model) = candidates.remove(0);
        Self::open_device(device, model, require_in)
    }

    fn open_device(device: Device<GlobalContext>, model: Model, require_in: bool) -> Result<Self> {
        let handle = device.open().context("open USB; check udev permissions and other applications")?;
        handle.set_auto_detach_kernel_driver(true)?;
        if handle.active_configuration()? == 0 { handle.set_active_configuration(1)?; }
        let config = device.active_config_descriptor()?;
        let mut candidates = Vec::new();
        for iface in config.interfaces() {
            for alt in iface.descriptors() {
                let outs: Vec<_> = alt.endpoint_descriptors().filter(|e| e.transfer_type() == TransferType::Bulk && e.direction() == Direction::Out).collect();
                let input = alt.endpoint_descriptors().find(|e| e.transfer_type() == TransferType::Bulk && e.direction() == Direction::In);
                let output = outs.iter().find(|e| e.address() & 0x0f == model.out()).or(outs.first());
                if let Some(out) = output {
                    if require_in && input.is_none() { continue; }
                    candidates.push((alt.interface_number(), alt.setting_number(), out.address(), input.as_ref().map(|e| e.address()), input.map(|e| e.max_packet_size() as usize).unwrap_or(64)));
                }
            }
        }
        candidates.sort_by_key(|c| (c.0 != 0, c.0, c.1));
        let (interface, alt, out, input, packet) = candidates.into_iter().next().context("no matching bulk endpoints")?;
        handle.claim_interface(interface).context("claim USB interface; stop CUPS/other printer applications if busy")?;
        // Construct RAII owner immediately so subsequent errors release the interface.
        let s = Self { handle, interface, out, input, read_len: if model == Model::Mip201 { 512.max(packet) } else { packet }, model };
        // GET_INTERFACE avoids resetting the already active alternate setting.
        let mut active = [0u8; 1];
        let n = s.handle.read_control(0x81, 0x0a, 0, u16::from(interface), &mut active, Duration::from_secs(2))?;
        ensure!(n == 1, "GET_INTERFACE returned {n} bytes");
        if active[0] != alt { s.handle.set_alternate_setting(interface, alt)?; }
        if model == Model::Mip201 {
            s.clear_halt(out).context("USB_RESYNC_FAILED: OUT toggle not synchronised; no print was sent")?;
            if let Some(ep) = input {
                let result = s.clear_halt(ep);
                if require_in { result.context("USB_RESYNC_FAILED: IN toggle not synchronised")?; }
            }
        }
        Ok(s)
    }

    fn clear_halt(&self, ep: u8) -> Result<()> {
        let mut last = None;
        for attempt in 0..3 {
            if attempt > 0 { thread::sleep(Duration::from_millis(50)); }
            match self.handle.clear_halt(ep) { Ok(()) => return Ok(()), Err(e) => last = Some(e) }
        }
        Err(last.unwrap().into())
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        // Do not retry a failed job: some data may already have been accepted.
        // One bulk request normally matches SDK transferOut's single logical payload.
        let mut remaining = data;
        while !remaining.is_empty() {
            let n = self.handle.write_bulk(self.out, remaining, Duration::from_secs(30))
                .context("USB write failed; delivery uncertain; do not blindly retry")?;
            ensure!(n > 0, "USB write made no progress");
            remaining = &remaining[n..];
        }
        Ok(())
    }

    // Arm a blocking libusb read in another thread before sending the query, as
    // SDK startUsbTransferIn does. The short scheduling grace is best-effort,
    // not a guarantee of URB submission; physical hardware validation is required.
    fn query(&self, command: &[u8], information: bool) -> Result<Vec<u8>> {
        let ep = self.input.context("no USB IN endpoint")?;
        thread::scope(|scope| {
            let (ready_tx, ready_rx) = mpsc::sync_channel(0);
            let reader = scope.spawn(move || -> Result<Vec<u8>> {
                ready_tx.send(())?;
                let wait = if information { Duration::from_secs(30) } else { Duration::from_secs(2) };
                let deadline = Instant::now() + wait;
                let mut received = Vec::new();
                loop {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    ensure!(!remaining.is_zero(), "USB response deadline exceeded");
                    let mut chunk = vec![0; self.read_len];
                    let n = self.handle.read_bulk(ep, &mut chunk, remaining)?;
                    received.extend_from_slice(&chunk[..n]);
                    ensure!(received.len() <= 64 * 1024, "USB response too large");
                    if !information && !received.is_empty() { return Ok(received); }
                    if information && valid_information(&received, self.model).is_some() {
                        if n < self.read_len { return Ok(received); }
                        // A full packet might have a continuation or a terminating ZLP.
                        loop {
                            let mut tail = vec![0; self.read_len];
                            match self.handle.read_bulk(ep, &mut tail, Duration::from_millis(150)) {
                                Ok(k) => {
                                    received.extend_from_slice(&tail[..k]);
                                    ensure!(received.len() <= 64 * 1024, "USB response too large");
                                    if k < self.read_len { return Ok(received); }
                                    ensure!(Instant::now() < deadline, "USB information deadline exceeded");
                                }
                                Err(rusb::Error::Timeout) => return Ok(received),
                                Err(e) => return Err(e.into()),
                            }
                        }
                    }
                }
            });
            ready_rx.recv()?;
            thread::sleep(Duration::from_millis(5));
            let written = self.write(command);
            let response = reader.join().map_err(|_| anyhow::anyhow!("USB reader panicked"))?;
            written?;
            response.context("USB read failed; session abandoned, response order unknown; reconnect if persistent")
        })
    }

    pub fn information(&self) -> Result<Value> {
        let bytes = self.query(&[0x1d, 0x49, 0x02], true)?;
        let text = valid_information(&bytes, self.model).context("invalid printer information")?;
        let t: Vec<_> = text.split_whitespace().collect();
        Ok(json!({"brand_name":t[0], "model_name":t[1], "boot_version":t[2], "firmware_version":t[3],
            "boot_checksum":t[4], "program_checksum":t[5], "print_length":t[6].parse::<u64>()?,
            "cutter_count":t[7].parse::<u64>()?, "paper_jam_count":t[8].parse::<u64>()?,
            "overheat_count":if self.model == Model::Mip001 { None } else { Some(t[9].parse::<u64>()?) },
            "mode":t[t.len()-1], "raw_text":text, "raw_hex":hex(&bytes)}))
    }

    pub fn status(&self) -> Result<Value> {
        // GS I is an unambiguous marker, drains earlier answers and waits for a busy
        // printer. Stop on any failure; do not guess whether a late byte is status.
        let _ = self.information()?;
        let bytes = self.query(&[0x10, 0x04, 0x02], false)?;
        ensure!(bytes.len() == 1, "expected one status byte; got {} (not interpreted)", bytes.len());
        Ok(parse_status(bytes[0]))
    }
}

pub fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }

fn valid_information(bytes: &[u8], model: Model) -> Option<String> {
    let clean: String = bytes.iter().copied().filter(|b| *b != 0 && (*b == b'\r' || *b == b'\n' || *b == b'\t' || (32..=126).contains(b))).map(char::from).collect();
    let start = clean.to_ascii_lowercase().rfind("nemonic")?;
    let text = clean[start..].trim();
    let t: Vec<_> = text.split_whitespace().collect();
    if t.len() != model.tokens() { return None; }
    if t[6..if model == Model::Mip001 { 9 } else { 10 }].iter().any(|s| s.parse::<u64>().is_err()) { return None; }
    Some(text.to_owned())
}

pub fn parse_status(raw: u8) -> Value {
    let cover = raw & 0x04 != 0;
    let heat = raw & 0x08 != 0;
    let paper = raw & 0x20 != 0;
    let jam = raw & 0x40 != 0;
    let (code, key) = if heat { (8,"OVERHEAT") } else if cover { (4,"COVER_OPENED") }
        else if paper { (2,"OUT_OF_PAPER") } else if jam { (16,"PAPER_JAM") } else { (0,"OK") };
    json!({"raw":raw,"raw_hex":format!("{raw:02x}"),"code":code,"key":key,
        "is_ok":code == 0,"is_cover_opened":cover,"is_out_of_paper":paper,"is_overheat":heat,"is_paper_jam":jam})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn status_bits_and_priority() {
        assert_eq!(parse_status(0x20)["key"], "OUT_OF_PAPER");
        assert_eq!(parse_status(0x6c)["key"], "OVERHEAT");
        assert_eq!(parse_status(0)["is_ok"], true);
    }
    #[test] fn marker_and_information() {
        let b = b"\x02nemonic MIP-201 1 2 aa bb 10 20 30 40 usb\r ";
        assert!(valid_information(b, Model::Mip201).is_some());
        assert!(valid_information(b, Model::Mip001).is_none());
        assert!(valid_information(b"nemonic short", Model::Mip201).is_none());
    }
}
