//! The connection: one socket, one reader thread, replies matched to requests.
//!
//! JDWP is not request/response on a quiet line. While a command is in flight the VM may send
//! events, replies may come back in an order the client did not ask in, and both directions
//! share one socket. So reading is a **dedicated thread**: it decodes every packet, hands
//! replies to whoever is waiting for that id, and pushes events onto a channel. Callers of
//! [`Client::send`] block on their own reply and nothing else.
//!
//! The consequence worth stating: a caller must never do JDWP work on the thread that drains
//! the event channel. A `Breakpoint` event arrives, the handler asks for the thread's frames,
//! and if that ask blocked the same thread the reply could never be delivered — the classic
//! reverse-channel deadlock, and the same one the Arbor shell hit going out-of-process.

use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::codec::IdSizes;
use crate::error::{JdwpError, Result};
use crate::event::{parse_composite, Composite};
use crate::packet::{handshake, read_packet, Packet, PacketKind};

/// How long a command waits for its reply before giving up. A suspended VM answers in
/// microseconds; this is here so a VM that has stopped answering fails a thread instead of
/// parking it for the life of the process.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// What a reply carried: its payload, or the VM's refusal.
type ReplySlot = Sender<std::result::Result<Vec<u8>, u16>>;

struct Shared {
    pending: Mutex<HashMap<u32, ReplySlot>>,
    sizes: Mutex<IdSizes>,
    alive: AtomicBool,
}

/// A connected debuggee.
///
/// Cheap to share (`Arc<Client>`): every method takes `&self`, and the only exclusive thing is
/// the write half of the socket, which is behind its own lock for exactly as long as a packet
/// takes to write.
pub struct Client {
    out: Mutex<TcpStream>,
    shared: Arc<Shared>,
    next_id: AtomicU32,
}

impl Client {
    /// Connect to a JVM started with `-agentlib:jdwp=transport=dt_socket,server=y,address=…`,
    /// shake hands, and learn its identifier widths.
    ///
    /// Returns the client and the event stream. The stream is a plain channel: whoever owns it
    /// decides what a session is, which is deliberately not this crate's business.
    pub fn attach(addr: impl ToSocketAddrs) -> Result<(Client, Receiver<Composite>)> {
        Client::from_stream(TcpStream::connect(addr)?)
    }

    /// Take over an already-connected socket — the other way round, where the **VM** does the
    /// connecting (`server=n,address=<our port>`) and the debugger was listening first.
    ///
    /// Worth having as well as [`attach`](Client::attach): with `server=y` the VM picks the
    /// port and announces it on its own stdout, so a launcher has to scrape a line of the
    /// program's output and race the program's own writes to it. Listening first means the
    /// port is known before the process exists, and there is no line to parse.
    pub fn from_stream(mut stream: TcpStream) -> Result<(Client, Receiver<Composite>)> {
        // Nagle would sit on a 20-byte command waiting for company. A debugger's round trips
        // are small and serial, which is the exact case it hurts.
        let _ = stream.set_nodelay(true);
        handshake(&mut stream)?;

        let reader = stream.try_clone()?;
        let shared = Arc::new(Shared {
            pending: Mutex::new(HashMap::new()),
            sizes: Mutex::new(IdSizes::default()),
            alive: AtomicBool::new(true),
        });
        let (events_tx, events_rx) = channel();
        let pump = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("jdwp-read".into())
            .spawn(move || pump_packets(reader, pump, events_tx))
            .map_err(JdwpError::Io)?;

        let client =
            Client { out: Mutex::new(stream), shared, next_id: AtomicU32::new(1) };
        // Before anything else: every other reply is unparseable without this.
        let sizes = crate::command::id_sizes(&client)?;
        *client.shared.sizes.lock().unwrap_or_else(|p| p.into_inner()) = sizes;
        Ok((client, events_rx))
    }

    /// The negotiated identifier widths. Valid from the moment [`attach`](Client::attach)
    /// returns.
    pub fn sizes(&self) -> IdSizes {
        *self.shared.sizes.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Whether the connection is still up. Goes false when the VM exits — which is a normal
    /// end to a debugging session, not an error.
    pub fn is_alive(&self) -> bool {
        self.shared.alive.load(Ordering::Acquire)
    }

    /// Drop the connection now, without asking the VM.
    ///
    /// [`dispose`](crate::command::dispose) is the polite ending and the one to prefer — it
    /// leaves the program running and unsuspended. This is for the case where the VM is not
    /// answering: it closes the socket under the reader thread, which ends it and wakes
    /// everything still waiting on a reply that is not coming.
    pub fn close(&self) {
        let out = self.out.lock().unwrap_or_else(|p| p.into_inner());
        let _ = out.shutdown(std::net::Shutdown::Both);
    }

    /// Send a command and wait for its reply's payload.
    ///
    /// `context` names the command in any error — `"Method.LineTable"` rather than the two
    /// numbers, because the numbers mean nothing to whoever reads the message.
    pub fn send(
        &self,
        set: u8,
        command: u8,
        payload: Vec<u8>,
        context: &'static str,
    ) -> Result<Vec<u8>> {
        if !self.is_alive() {
            return Err(JdwpError::Io(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "the debugged VM is gone",
            )));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        // Registered BEFORE the write: the reply can be back before `write_all` returns.
        self.shared.pending.lock().unwrap_or_else(|p| p.into_inner()).insert(id, tx);

        let bytes = Packet::command(id, set, command, payload).to_bytes();
        let write = {
            let mut out = self.out.lock().unwrap_or_else(|p| p.into_inner());
            out.write_all(&bytes).and_then(|()| out.flush())
        };
        if let Err(e) = write {
            self.shared.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
            return Err(JdwpError::Io(e));
        }

        match rx.recv_timeout(REPLY_TIMEOUT) {
            Ok(Ok(data)) => Ok(data),
            Ok(Err(code)) => Err(JdwpError::Vm { code, context }),
            Err(_) => {
                self.shared.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
                Err(JdwpError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("the VM did not answer {context}"),
                )))
            }
        }
    }
}

/// The reader thread: every packet, forever, until the socket closes.
fn pump_packets(mut reader: TcpStream, shared: Arc<Shared>, events: Sender<Composite>) {
    loop {
        let packet = match read_packet(&mut reader) {
            Ok(p) => p,
            // Any read failure ends the session: the VM exited, or the socket broke. Both
            // mean nothing more is coming.
            Err(_) => break,
        };
        match packet.kind {
            PacketKind::Reply { error } => {
                let slot =
                    shared.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&packet.id);
                if let Some(tx) = slot {
                    let _ = tx.send(if error == 0 { Ok(packet.data) } else { Err(error) });
                }
            }
            PacketKind::Command { set: 64, command: 100 } => {
                let sizes = *shared.sizes.lock().unwrap_or_else(|p| p.into_inner());
                if let Ok(composite) = parse_composite(&packet.data, sizes) {
                    // A closed receiver means nobody is debugging any more; keep draining the
                    // socket anyway, or the VM blocks writing to a full pipe.
                    let _ = events.send(composite);
                }
            }
            // The VM sends no other commands. Ignore rather than assume.
            PacketKind::Command { .. } => {}
        }
    }
    shared.alive.store(false, Ordering::Release);
    // Wake everyone still waiting: their reply is never coming.
    shared.pending.lock().unwrap_or_else(|p| p.into_inner()).clear();
}
