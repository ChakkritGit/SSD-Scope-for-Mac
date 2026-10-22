//! Allowlisted read-only SAT commands and strict BOT response parsing.
//! This module performs no hardware I/O. USB capture lives in the separate helper.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SatMode {
    Sat16,
    Sat12,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadCommand {
    Identify,
    SmartData,
    SmartThresholds,
    SmartStatus,
    RequestSense,
}

pub struct Request {
    cdb: [u8; 16],
    cdb_len: u8,
    data_len: u32,
}
impl Request {
    pub fn new(command: ReadCommand, mode: SatMode) -> Self {
        let mut cdb = [0; 16];
        if command == ReadCommand::RequestSense {
            cdb[0] = 0x03;
            cdb[1] = 1;
            cdb[4] = 64;
            return Self {
                cdb,
                cdb_len: 6,
                data_len: 64,
            };
        }
        let data = command != ReadCommand::SmartStatus;
        let (ata, feature) = match command {
            ReadCommand::Identify => (0xec, 0),
            ReadCommand::SmartData => (0xb0, 0xd0),
            ReadCommand::SmartThresholds => (0xb0, 0xd1),
            ReadCommand::SmartStatus => (0xb0, 0xda),
            ReadCommand::RequestSense => unreachable!(),
        };
        let protocol = if data { 4 << 1 } else { 3 << 1 };
        let flags = if data { 0x0e } else { 0x20 }; // data-in/blocks/count or CK_COND
        let (mid, high) = if ata == 0xb0 { (0x4f, 0xc2) } else { (0, 0) };
        let cdb_len = match mode {
            SatMode::Sat16 => {
                cdb[0] = 0x85;
                cdb[1] = protocol;
                cdb[2] = flags;
                cdb[4] = feature;
                cdb[6] = u8::from(data);
                cdb[10] = mid;
                cdb[12] = high;
                cdb[14] = ata;
                16
            }
            SatMode::Sat12 => {
                cdb[0] = 0xa1;
                cdb[1] = protocol;
                cdb[2] = flags;
                cdb[3] = feature;
                cdb[4] = u8::from(data);
                cdb[6] = mid;
                cdb[7] = high;
                cdb[9] = ata;
                12
            }
        };
        Self {
            cdb,
            cdb_len,
            data_len: if data { 512 } else { 0 },
        }
    }
    pub fn expected_len(&self) -> usize {
        self.data_len as usize
    }
    pub fn cdb(&self) -> &[u8] {
        &self.cdb[..usize::from(self.cdb_len)]
    }
    pub fn cbw(&self, tag: u32) -> [u8; 31] {
        let mut bytes = [0; 31];
        bytes[..4].copy_from_slice(b"USBC");
        bytes[4..8].copy_from_slice(&tag.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.data_len.to_le_bytes());
        bytes[12] = 0x80; // every command is data-in or non-data; no disk data-out
        bytes[14] = self.cdb_len;
        bytes[15..].copy_from_slice(&self.cdb);
        bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Passed,
    CommandFailed,
}

pub enum ReadError {
    Stall,
    Failed(anyhow::Error),
}
pub trait BulkIo {
    fn check(&self) -> Result<()>;
    fn write_command(&mut self, bytes: &[u8]) -> Result<usize>;
    fn read_input(&mut self, bytes: &mut [u8]) -> std::result::Result<usize, ReadError>;
    fn clear_input_halt(&mut self) -> Result<()>;
}
/// One bounded BOT transaction. A malformed status aborts the session; no reset/retry loop.
pub fn transfer(io: &mut impl BulkIo, request: Request, tag: u32) -> Result<(Status, Vec<u8>)> {
    io.check()?;
    let cbw = request.cbw(tag);
    ensure!(io.write_command(&cbw)? == 31, "Short BOT command write");
    let mut data = vec![0; request.expected_len()];
    let mut early_status = None;
    if !data.is_empty() {
        io.check()?;
        match io.read_input(&mut data) {
            Ok(n) => {
                ensure!(n <= data.len(), "Impossible USB read length");
                data.truncate(n);
                if n == 13 && data.starts_with(b"USBS") {
                    early_status = Some(std::mem::take(&mut data));
                }
            }
            Err(ReadError::Stall) => {
                io.clear_input_halt()?;
                data.clear();
            }
            Err(ReadError::Failed(e)) => return Err(e),
        }
    }
    let status_bytes = if let Some(bytes) = early_status {
        bytes
    } else {
        io.check()?;
        let mut bytes = vec![0; 13];
        let n = match io.read_input(&mut bytes) {
            Ok(n) => n,
            Err(ReadError::Stall) => {
                io.clear_input_halt()?;
                io.check()?;
                match io.read_input(&mut bytes) {
                    Ok(n) => n,
                    Err(ReadError::Stall) => bail!("Repeated BOT status stall"),
                    Err(ReadError::Failed(e)) => return Err(e),
                }
            }
            Err(ReadError::Failed(e)) => return Err(e),
        };
        ensure!(n <= bytes.len(), "Impossible BOT status length");
        bytes.truncate(n);
        bytes
    };
    let status = parse_csw(&status_bytes, tag, request.expected_len(), data.len())?;
    Ok((status, data))
}
pub fn parse_csw(bytes: &[u8], tag: u32, expected: usize, received: usize) -> Result<Status> {
    ensure!(
        bytes.len() == 13 && &bytes[..4] == b"USBS",
        "Invalid BOT status signature/length"
    );
    ensure!(
        u32::from_le_bytes(bytes[4..8].try_into()?) == tag,
        "BOT tag mismatch"
    );
    let residue = u32::from_le_bytes(bytes[8..12].try_into()?) as usize;
    ensure!(
        received <= expected && residue <= expected && received + residue == expected,
        "BOT data length/residue mismatch"
    );
    match bytes[12] {
        0 => Ok(Status::Passed),
        1 => Ok(Status::CommandFailed),
        _ => bail!("BOT phase error or invalid status; stop this session"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AtaHealth {
    Passed,
    Failing,
}
pub fn status_from_sense(bytes: &[u8]) -> Option<AtaHealth> {
    if bytes.len() < 8
        || bytes[0] & 0x7f != 0x72
        || bytes[1] & 0x0f > 1
        || bytes[2] != 0
        || !matches!(bytes[3], 0 | 0x1d)
    {
        return None;
    }
    let end = 8 + usize::from(bytes[7]);
    if end > bytes.len() {
        return None;
    }
    let mut offset = 8;
    while offset + 2 <= end {
        let size = 2 + usize::from(bytes[offset + 1]);
        if offset + size > end {
            return None;
        }
        if bytes[offset] == 9 && size >= 14 {
            let d = &bytes[offset..offset + size];
            // A successful task-file response must be ready, without BSY/DF/DRQ/ERR.
            if d[13] & 0x40 == 0 || d[13] & 0xa9 != 0 || d[3] != 0 {
                return None;
            }
            return match (d[9], d[11]) {
                (0x4f, 0xc2) => Some(AtaHealth::Passed),
                (0xf4, 0x2c) => Some(AtaHealth::Failing),
                _ => None,
            };
        }
        offset += size;
    }
    None
}

pub fn identify_model(bytes: &[u8]) -> Result<String> {
    ensure!(
        bytes.len() == 512 && bytes[..2] != [0, 0] && bytes[..2] != [255, 255],
        "Invalid ATA identity"
    );
    if bytes[510] == 0xa5 {
        checksum(bytes)?;
    }
    let mut model = Vec::with_capacity(40);
    for word in bytes[54..94].as_chunks::<2>().0 {
        model.extend([word[1], word[0]]);
    }
    ensure!(
        model
            .iter()
            .all(|v| v.is_ascii_graphic() || *v == b' ' || *v == 0),
        "Invalid ATA model text"
    );
    let model = String::from_utf8(model)?
        .trim_matches([' ', '\0'])
        .to_owned();
    ensure!(!model.is_empty(), "Missing ATA model");
    Ok(model)
}
pub fn checksum(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() == 512 && bytes.iter().fold(0u8, |s, b| s.wrapping_add(*b)) == 0,
        "ATA sector length/checksum mismatch"
    );
    Ok(())
}
pub fn validate_attributes(bytes: &[u8]) -> Result<()> {
    checksum(bytes)?;
    ensure!(
        u16::from_le_bytes([bytes[0], bytes[1]]) != 0 && bytes.iter().any(|v| *v != 0),
        "Empty SMART sector"
    );
    let mut seen = [false; 256];
    let mut count = 0;
    for row in bytes[2..362].as_chunks::<12>().0 {
        let id = usize::from(row[0]);
        if id != 0 {
            ensure!(!seen[id], "Duplicate SMART attribute");
            seen[id] = true;
            count += 1;
        }
    }
    ensure!(count > 0, "No SMART attributes returned");
    Ok(())
}
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").expect("string formatting");
    }
    s
}
pub fn unhex(text: &str, max: usize) -> Result<Vec<u8>> {
    ensure!(
        text.len().is_multiple_of(2) && text.len() / 2 <= max && text.is_ascii(),
        "Invalid hexadecimal length"
    );
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let digit = |b: u8| {
                (b as char)
                    .to_digit(16)
                    .ok_or_else(|| anyhow::anyhow!("Invalid hexadecimal digit"))
            };
            Ok((digit(pair[0])? * 16 + digit(pair[1])?) as u8)
        })
        .collect()
}
pub fn redacted_identity(bytes: &[u8]) -> Result<Vec<u8>> {
    identify_model(bytes)?;
    let mut result = bytes.to_vec();
    result[20..40].fill(0); // ATA serial
    result[216..224].fill(0); // WWN
    if result[510] == 0xa5 {
        result[511] = 0u8.wrapping_sub(result[..511].iter().fold(0u8, |s, b| s.wrapping_add(*b)));
    }
    Ok(result)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCapture {
    pub version: u32,
    pub device: String,
    pub usb_registry_id: u64,
    pub captured_unix: u64,
    pub mode: SatMode,
    pub model: String,
    pub identity_redacted_hex: String,
    pub smart_data_hex: String,
    pub thresholds_hex: Option<String>,
    pub status_sense_hex: Option<String>,
    pub driver_reattached: bool,
    pub notes: Vec<String>,
}
impl RawCapture {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && physical_id(&self.device),
            "Unsupported capture schema/device"
        );
        ensure!(
            self.notes.len() <= 16 && self.notes.iter().all(|n| n.len() <= 2048),
            "Oversized capture notes"
        );
        let identity = unhex(&self.identity_redacted_hex, 512)?;
        ensure!(
            identify_model(&identity)? == self.model,
            "Model does not match captured identity"
        );
        ensure!(
            identity[20..40]
                .iter()
                .chain(identity[216..224].iter())
                .all(|b| *b == 0),
            "Identity identifiers are not redacted"
        );
        validate_attributes(&unhex(&self.smart_data_hex, 512)?)?;
        if let Some(data) = &self.thresholds_hex {
            checksum(&unhex(data, 512)?)?;
        }
        if let Some(data) = &self.status_sense_hex {
            unhex(data, 64)?;
        }
        Ok(())
    }
    pub fn health(&self) -> Result<Option<AtaHealth>> {
        self.validate()?;
        Ok(self
            .status_sense_hex
            .as_deref()
            .map(|s| unhex(s, 64))
            .transpose()?
            .as_deref()
            .and_then(status_from_sense))
    }
    /// A replay of captured read responses for the separately licensed smartctl decoder.
    pub fn replay(&self) -> Result<String> {
        self.validate()?;
        let mut text = String::new();
        for (name, data) in [
            ("IDENTIFY DEVICE", Some(self.identity_redacted_hex.as_str())),
            (
                "SMART READ ATTRIBUTE VALUES",
                Some(self.smart_data_hex.as_str()),
            ),
            (
                "SMART READ ATTRIBUTE THRESHOLDS",
                self.thresholds_hex.as_deref(),
            ),
        ] {
            writeln!(text, "REPORT-IOCTL: DeviceFD=0 Command={name}")?;
            writeln!(
                text,
                "REPORT-IOCTL: DeviceFD=0 Command={name} returned {}",
                if data.is_some() { "0" } else { "-1 errno=95" }
            )?;
            if let Some(data) = data {
                writeln!(text, "===== [{name}] DATA START (BASE-16) =====")?;
                for (i, row) in unhex(data, 512)?.as_chunks::<16>().0.iter().enumerate() {
                    write!(text, "{:03}-{:03}: ", i * 16, i * 16 + 15)?;
                    for b in row {
                        write!(text, "{b:02x} ")?;
                    }
                    writeln!(text)?;
                }
                writeln!(text, "===== [{name}] DATA END (BASE-16) =====")?;
            }
        }
        let health = self.health()?;
        for (name, code) in [
            (
                "SMART STATUS",
                if health.is_some() { "0" } else { "-1 errno=95" },
            ),
            (
                "SMART STATUS CHECK",
                match health {
                    Some(AtaHealth::Passed) => "0",
                    Some(AtaHealth::Failing) => "1",
                    None => "-1 errno=95",
                },
            ),
        ] {
            writeln!(text, "REPORT-IOCTL: DeviceFD=0 Command={name}")?;
            writeln!(
                text,
                "REPORT-IOCTL: DeviceFD=0 Command={name} returned {code}"
            )?;
        }
        Ok(text)
    }
}
pub fn physical_id(id: &str) -> bool {
    id.strip_prefix("disk")
        .is_some_and(|s| !s.is_empty() && s.len() <= 8 && s.bytes().all(|c| c.is_ascii_digit()))
}
