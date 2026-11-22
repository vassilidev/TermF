//! CTAPHID (CTAP 2.1 §11.2) over USB HID: 64-byte reports, a channel per conversation, keepalives while the key
//! waits for a touch.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use termf_sync::crypto;

use super::ctap::{Device, Status, TIMEOUT};

const REPORT: usize = 64;
const BROADCAST: [u8; 4] = [0xff; 4];
const INIT: u8 = 0x86;
const CBOR: u8 = 0x90;
const KEEPALIVE: u8 = 0xbb;
const ERROR: u8 = 0xbf;
/// Longer than a key's own wait for a touch (30 s for most), so the key says it first.
const WAIT: Duration = Duration::from_secs(40);

/// Sends and receives one report; `Ok(None)` when nothing came before `timeout`.
pub trait Reports {
    fn send(&mut self, report: &[u8; REPORT]) -> Result<()>;
    fn receive(&mut self, timeout: Duration) -> Result<Option<[u8; REPORT]>>;
}

pub struct Channel<R> {
    reports: R,
    cid: [u8; 4],
}

impl<R: Reports> Channel<R> {
    pub fn open(reports: R) -> Result<Self> {
        let mut channel = Self {
            reports,
            cid: BROADCAST,
        };
        let nonce: [u8; 8] = crypto::random();
        let answer = channel.transact(INIT, &nonce)?;
        ensure!(
            answer.len() >= 17 && answer[..8] == nonce,
            "the security key did not open a channel"
        );
        ensure!(
            answer[16] & 0x04 != 0,
            "this security key does not speak CTAP2"
        );
        channel.cid.copy_from_slice(&answer[8..12]);
        Ok(channel)
    }

    fn transact(&mut self, command: u8, data: &[u8]) -> Result<Vec<u8>> {
        ensure!(data.len() <= 7609, "message too long for the security key");
        let mut report = [0u8; REPORT];
        report[..4].copy_from_slice(&self.cid);
        report[4] = command;
        report[5..7].copy_from_slice(&(data.len() as u16).to_be_bytes());
        let first = data.len().min(REPORT - 7);
        report[7..7 + first].copy_from_slice(&data[..first]);
        self.reports.send(&report)?;
        for (seq, chunk) in data[first..].chunks(REPORT - 5).enumerate() {
            let mut report = [0u8; REPORT];
            report[..4].copy_from_slice(&self.cid);
            report[4] = seq as u8;
            report[5..5 + chunk.len()].copy_from_slice(chunk);
            self.reports.send(&report)?;
        }
        self.receive(command)
    }

    fn receive(&mut self, command: u8) -> Result<Vec<u8>> {
        let deadline = Instant::now() + WAIT;
        let next = |reports: &mut R| -> Result<[u8; REPORT]> {
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                ensure!(!left.is_zero(), TIMEOUT);
                if let Some(report) = reports.receive(left.min(Duration::from_millis(500)))? {
                    return Ok(report);
                }
            }
        };
        let first = loop {
            let report = next(&mut self.reports)?;
            if report[..4] != self.cid {
                continue;
            }
            match report[4] {
                KEEPALIVE => continue,
                ERROR => bail!(
                    "the security key reported a transport error (0x{:02x})",
                    report[7]
                ),
                c if c == command => break report,
                _ => bail!("the security key answered out of turn"),
            }
        };
        let len = usize::from(u16::from_be_bytes([first[5], first[6]]));
        let mut data = first[7..].to_vec();
        let mut seq = 0u8;
        while data.len() < len {
            let report = next(&mut self.reports)?;
            if report[..4] != self.cid {
                continue;
            }
            ensure!(report[4] == seq, "the security key lost a packet");
            seq += 1;
            data.extend_from_slice(&report[5..]);
        }
        data.truncate(len);
        Ok(data)
    }
}

impl<R: Reports> Device for Channel<R> {
    fn cbor(&mut self, command: u8, request: &[u8]) -> Result<Vec<u8>> {
        let answer = self.transact(CBOR, &[&[command][..], request].concat())?;
        let (&status, body) = answer
            .split_first()
            .context("the security key sent an empty answer")?;
        ensure!(status == 0, Status(status));
        Ok(body.to_vec())
    }
}

#[cfg(not(windows))]
pub mod usb {
    use super::*;

    /// FIDO's HID usage page and its CTAPHID usage.
    const USAGE_PAGE: u16 = 0xf1d0;
    const USAGE: u16 = 0x01;

    pub struct Hid(hidapi::HidDevice);

    impl Reports for Hid {
        fn send(&mut self, report: &[u8; REPORT]) -> Result<()> {
            // The leading 0 is the report number, which FIDO keys do not use.
            let sent = self.0.write(&[&[0u8][..], report].concat())?;
            ensure!(sent >= REPORT, "the security key took a partial report");
            Ok(())
        }

        fn receive(&mut self, timeout: Duration) -> Result<Option<[u8; REPORT]>> {
            let mut report = [0u8; REPORT];
            let millis = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
            match self.0.read_timeout(&mut report, millis)? {
                0 => Ok(None),
                _ => Ok(Some(report)),
            }
        }
    }

    /// Every security key plugged in, each on its own channel.
    pub fn devices() -> Result<Vec<Channel<Hid>>> {
        let api = hidapi::HidApi::new().context("cannot list the USB devices")?;
        let mut keys = Vec::new();
        let mut refused = None;
        for info in api.device_list() {
            if info.usage_page() != USAGE_PAGE || info.usage() != USAGE {
                continue;
            }
            match info.open_device(&api) {
                Ok(device) => keys.push(Channel::open(Hid(device))?),
                Err(e) => refused = Some(e),
            }
        }
        if keys.is_empty()
            && let Some(e) = refused
        {
            bail!(
                "cannot open the security key ({e}); on Linux, the system needs udev's rules for FIDO keys (systemd 244 or later, or the libfido2 package)"
            );
        }
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    /// A key that echoes CBOR requests back after a keepalive, in reports cut as CTAPHID cuts them.
    struct Echo {
        cid: [u8; 4],
        sent: Vec<[u8; REPORT]>,
        replies: VecDeque<[u8; REPORT]>,
    }

    impl Echo {
        fn reply(&mut self, command: u8, data: &[u8]) {
            let mut report = [0u8; REPORT];
            report[..4].copy_from_slice(&self.cid);
            report[4] = command;
            report[5..7].copy_from_slice(&(data.len() as u16).to_be_bytes());
            let first = data.len().min(REPORT - 7);
            report[7..7 + first].copy_from_slice(&data[..first]);
            self.replies.push_back(report);
            for (seq, chunk) in data[first..].chunks(REPORT - 5).enumerate() {
                let mut report = [0u8; REPORT];
                report[..4].copy_from_slice(&self.cid);
                report[4] = seq as u8;
                report[5..5 + chunk.len()].copy_from_slice(chunk);
                self.replies.push_back(report);
            }
        }
    }

    impl Reports for &mut Echo {
        fn send(&mut self, report: &[u8; REPORT]) -> Result<()> {
            self.sent.push(*report);
            let first = &self.sent[0];
            let len = usize::from(u16::from_be_bytes([first[5], first[6]]));
            let mut data = first[7..].to_vec();
            for r in &self.sent[1..] {
                data.extend_from_slice(&r[5..]);
            }
            if data.len() < len {
                return Ok(());
            }
            data.truncate(len);
            let command = first[4];
            self.sent.clear();
            if command == INIT {
                let mut answer = data.clone();
                answer.extend([9, 9, 9, 9, 2, 1, 0, 0, 0x04 | 0x01]);
                self.reply(INIT, &answer);
                self.cid = [9; 4];
            } else {
                // Another channel's traffic and a keepalive first: both are skipped.
                let mut other = [0u8; REPORT];
                other[..4].copy_from_slice(&[7; 4]);
                other[4] = CBOR;
                self.replies.push_back(other);
                let mut keepalive = [0u8; REPORT];
                keepalive[..4].copy_from_slice(&self.cid);
                keepalive[4] = KEEPALIVE;
                keepalive[7] = 2;
                self.replies.push_back(keepalive);
                let mut answer = vec![0u8];
                answer.extend_from_slice(&data[1..]);
                self.reply(CBOR, &answer);
            }
            Ok(())
        }

        fn receive(&mut self, _: Duration) -> Result<Option<[u8; REPORT]>> {
            Ok(self.replies.pop_front())
        }
    }

    #[test]
    fn cuts_and_joins_messages_in_reports() {
        let mut echo = Echo {
            cid: BROADCAST,
            sent: Vec::new(),
            replies: VecDeque::new(),
        };
        let mut channel = Channel::open(&mut echo).unwrap();
        assert_eq!(channel.cid, [9; 4]);
        let long: Vec<u8> = (0..300u32).map(|i| i as u8).collect();
        assert_eq!(channel.cbor(0x04, &long).unwrap(), long);
        assert_eq!(channel.cbor(0x04, b"").unwrap(), b"");
    }

    #[test]
    fn says_a_ctap_error_in_words() {
        struct Refusing;
        impl Reports for Refusing {
            fn send(&mut self, _: &[u8; REPORT]) -> Result<()> {
                Ok(())
            }
            fn receive(&mut self, _: Duration) -> Result<Option<[u8; REPORT]>> {
                let mut r = [0u8; REPORT];
                r[..4].copy_from_slice(&[1; 4]);
                r[4] = CBOR;
                r[6] = 1;
                r[7] = 0x31;
                Ok(Some(r))
            }
        }
        let mut channel = Channel {
            reports: Refusing,
            cid: [1; 4],
        };
        let err = channel.cbor(0x02, b"").unwrap_err();
        assert_eq!(err.to_string(), "wrong PIN for the security key");
    }
}
