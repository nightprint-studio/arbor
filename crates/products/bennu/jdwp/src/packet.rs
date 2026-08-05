//! The wire frame: the handshake, and the eleven-byte header in front of every packet.
//!
//! ```text
//! length (4)  id (4)  flags (1)  ┬─ command: set (1) command (1)
//!                                └─ reply:   error code (2)
//! ```
//!
//! `length` counts the header, so the payload is `length - 11`. `id` is the client's, echoed
//! by the reply — which is how replies are matched to requests on a connection where they may
//! come back out of order, and where events arrive in between as command packets *from* the
//! VM.

use std::io::{Read, Write};

use crate::error::{JdwpError, Result};

/// The fourteen bytes each side sends before anything else. Both send it; both must see it.
pub const HANDSHAKE: &[u8; 14] = b"JDWP-Handshake";

/// Size of every packet header.
pub const HEADER_LEN: usize = 11;

/// Set in `flags` when the packet is a reply rather than a command.
pub const FLAG_REPLY: u8 = 0x80;

/// A packet, either direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub id: u32,
    pub kind: PacketKind,
    pub data: Vec<u8>,
}

/// What the last two header bytes meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketKind {
    /// A command. From the client it is a request; from the VM it is an event.
    Command { set: u8, command: u8 },
    /// A reply to the command with the same `id`. A non-zero `error` means the payload is
    /// empty and the command failed.
    Reply { error: u16 },
}

impl Packet {
    pub fn command(id: u32, set: u8, command: u8, data: Vec<u8>) -> Self {
        Packet { id, kind: PacketKind::Command { set, command }, data }
    }

    /// Whether this is a composite event packet — the only command the VM sends.
    pub fn is_event(&self) -> bool {
        matches!(self.kind, PacketKind::Command { set: 64, command: 100 })
    }

    /// The header + payload, ready for the socket.
    pub fn to_bytes(&self) -> Vec<u8> {
        let len = (HEADER_LEN + self.data.len()) as u32;
        let mut out = Vec::with_capacity(len as usize);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.id.to_be_bytes());
        match self.kind {
            PacketKind::Command { set, command } => {
                out.push(0);
                out.push(set);
                out.push(command);
            }
            PacketKind::Reply { error } => {
                out.push(FLAG_REPLY);
                out.extend_from_slice(&error.to_be_bytes());
            }
        }
        out.extend_from_slice(&self.data);
        out
    }

    /// Parse one packet from `header` + its payload.
    pub fn from_parts(header: [u8; HEADER_LEN], data: Vec<u8>) -> Self {
        let id = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
        let kind = if header[8] & FLAG_REPLY != 0 {
            PacketKind::Reply { error: u16::from_be_bytes([header[9], header[10]]) }
        } else {
            PacketKind::Command { set: header[9], command: header[10] }
        };
        Packet { id, kind, data }
    }
}

/// Send the handshake and require the same back. Until this succeeds the VM ignores
/// everything, so a client that skips it simply hangs — which is why this is not optional and
/// not lazy.
pub fn handshake(stream: &mut (impl Read + Write)) -> Result<()> {
    stream.write_all(HANDSHAKE)?;
    stream.flush()?;
    let mut back = [0u8; 14];
    stream.read_exact(&mut back)?;
    if &back != HANDSHAKE {
        return Err(JdwpError::Protocol(format!(
            "not a JDWP endpoint — it answered the handshake with {:?}",
            String::from_utf8_lossy(&back)
        )));
    }
    Ok(())
}

/// Read exactly one packet, blocking until it is whole.
pub fn read_packet(stream: &mut impl Read) -> Result<Packet> {
    let mut header = [0u8; HEADER_LEN];
    stream.read_exact(&mut header)?;
    let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    if len < HEADER_LEN {
        return Err(JdwpError::Protocol(format!("packet claims to be {len} bytes long")));
    }
    let mut data = vec![0u8; len - HEADER_LEN];
    stream.read_exact(&mut data)?;
    Ok(Packet::from_parts(header, data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn a_command_round_trips_through_its_bytes() {
        let packet = Packet::command(7, 1, 7, vec![1, 2, 3]);
        let bytes = packet.to_bytes();
        assert_eq!(&bytes[..4], &[0, 0, 0, 14]); // 11 header + 3 payload
        assert_eq!(bytes[8], 0); // not a reply
        let back = read_packet(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(back, packet);
    }

    #[test]
    fn a_reply_carries_its_error_code_where_a_command_carries_its_number() {
        let bytes =
            Packet { id: 3, kind: PacketKind::Reply { error: 112 }, data: vec![] }.to_bytes();
        let back = read_packet(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(back.kind, PacketKind::Reply { error: 112 });
    }

    #[test]
    fn the_vms_own_command_is_recognised_as_an_event() {
        assert!(Packet::command(0, 64, 100, vec![]).is_event());
        assert!(!Packet::command(0, 1, 7, vec![]).is_event());
    }

    #[test]
    fn a_wrong_handshake_says_so_instead_of_hanging() {
        // A socket that is not a JDWP agent — an HTTP server on the port you guessed.
        let mut fake = Cursor::new(b"HTTP/1.1 404 ".to_vec());
        let mut buf: Vec<u8> = Vec::new();
        struct Duplex<'a>(&'a mut Cursor<Vec<u8>>, &'a mut Vec<u8>);
        impl Read for Duplex<'_> {
            fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
                self.0.read(b)
            }
        }
        impl Write for Duplex<'_> {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.1.write(b)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut duplex = Duplex(&mut fake, &mut buf);
        let err = handshake(&mut duplex);
        assert!(matches!(err, Err(JdwpError::Protocol(_))));
    }

    #[test]
    fn a_short_packet_header_is_rejected() {
        let bytes = vec![0, 0, 0, 4, 0, 0, 0, 1, 0, 1, 1];
        assert!(matches!(
            read_packet(&mut Cursor::new(bytes)),
            Err(JdwpError::Protocol(_))
        ));
    }
}
