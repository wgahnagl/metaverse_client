use actix::prelude::*;
use actix_rt::time;
use bincode;
use log::{error, info, warn};
use metaverse_messages::packet::MessageType;
use metaverse_messages::packet::Packet;
use metaverse_messages::packet_ack::PacketAck;
use metaverse_messages::packet_types::PacketType;
use metaverse_messages::ui_events::UiEventTypes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::net::UdpSocket;
use tokio::sync::{oneshot, Notify};
use tokio::time::Duration;
use uuid::Uuid;

use metaverse_messages::errors::{AckError, SessionError};

const ACK_ATTEMPTS: i8 = 3;
const ACK_TIMEOUT: Duration = Duration::from_secs(1);

/// This is the mailbox for handling packets and sessions in the client
#[derive(Debug)]
pub struct Mailbox {
    /// the client socket for UDP connections
    pub client_socket: u16,
    /// UDS socket for connecting mailbox to the UI
    pub server_to_ui_socket: Option<ServerToUiSocket>,

    /// queue of ack packets to handle
    pub ack_queue: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,

    /// global number of received packets
    pub packet_sequence_number: Arc<Mutex<u32>>,
    /// state of the mailbox. If it is running or not.
    pub state: Arc<Mutex<ServerState>>,
    /// notify for etablishing when it begins running
    pub notify: Arc<Notify>,
    /// Session information for after login
    pub session: Option<Session>,

    /// the global number of packets that have been sent to the UI
    pub sent_packet_count: u16,

    /// the global ping information
    pub ping_info: PingInfo,
}

/// Session of the user
#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct Session {
    /// url of the server where the UDP session is connected to
    pub url: String,
    /// socket of the server where the UDP session is connected to
    pub server_socket: u16,
    /// agent ID of the user
    pub agent_id: Uuid,
    /// session ID of the user
    pub session_id: Uuid,
    /// the running UDP socket attached to the session  
    pub socket: Option<Arc<UdpSocket>>,
}

/// UDS socket for communicating from the server to the UI
#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct ServerToUiSocket {
    /// the path of the UDS socket on the machine
    pub socket_path: PathBuf,
}

/// Format for sending a serialized message from the mailbox to the UI.
#[derive(Debug, Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub struct UiMessage {
    /// Type of message, for decoding in the UI
    pub message_type: UiEventTypes,
    /// Which number in a series of messages is it
    pub sequence_number: u16,
    /// how mant messages are there in total
    pub total_packet_number: u16,
    /// for serializing
    pub packet_number: u16,
    /// the encoded message to be decoded by the UI
    pub message: Vec<u8>,
}
impl UiMessage {
    /// Convert the struct into bytes using JSON serialization
    pub fn as_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).expect("Failed to serialize UiMessage")
    }

    /// Convert bytes back into a `UiMessage` struct
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }
    /// create a new UiMessage
    pub fn new(message_type: UiEventTypes, message: Vec<u8>) -> UiMessage {
        UiMessage {
            message_type,
            message,
            // these will get handled later
            sequence_number: 0,
            total_packet_number: 0,
            packet_number: 0,
        }
    }
}

#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct PingInfo {
    pub ping_number: u8,
    pub ping_latency: Duration,
    pub last_ping: time::Instant,
}
#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct Ping;

#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct Pong;

/// The state of the Mailbox
#[derive(Debug, Clone, PartialEq)]
pub enum ServerState {
    /// The mailbox starts in the Starting state
    Starting,
    /// The mailbox is running
    Running,
    /// The mailbox is preparing to stop
    Stopping,
    /// the mailbox is stopped
    Stopped,
}

impl Mailbox {
    /// Start_udp_read is for reading packets coming from the external server
    async fn start_udp_read(
        ack_queue: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
        sock: Arc<UdpSocket>,
        mailbox_address: Addr<Mailbox>,
    ) {
        let mut buf = [0; 1024];
        loop {
            match sock.recv_from(&mut buf).await {
                Ok((size, _addr)) => {
                    //info!("Received {} bytes from {:?}", size, addr);

                    let packet = match Packet::from_bytes(&buf[..size]) {
                        Ok(packet) => packet,
                        Err(_) => {
                            continue;
                        }
                    };
                    info!(
                        "received packet: {:?}, {:?}",
                        packet.header.id, packet.header.frequency
                    );

                    if packet.header.reliable {
                        match mailbox_address
                            .send(Packet::new_packet_ack(PacketAck {
                                packet_ids: vec![packet.header.sequence_number],
                            }))
                            .await
                        {
                            Ok(_) => println!("ack sent"),
                            Err(_) => println!("ack failed to send"),
                        };
                    }

                    match &packet.body {
                        PacketType::PacketAck(data) => {
                            let mut queue = ack_queue.lock().unwrap();
                            for id in data.packet_ids.clone() {
                                if let Some(sender) = queue.remove(&id) {
                                    let _ = sender.send(());
                                } else {
                                    println!("No pending ack found for request ID: {}", id);
                                }
                            }
                        }
                        PacketType::CompletePingCheck(_) => {
                            if let Err(e) = mailbox_address.send(Pong).await{
                                warn!("failed to handle pong {:?}", e)
                            };
                        }
                        _ => {}
                    }
                    if let MessageType::Event = &packet.body.message_type() {
                        if let Err(e) = mailbox_address
                            .send(UiMessage::new(
                                packet.body.ui_event(),
                                packet.body.to_bytes(),
                            ))
                            .await
                        {
                            warn!("failed to send to ui: {:?}", e)
                        };
                    }
                }
                Err(e) => {
                    eprintln!("Failed to receive data: {}", e);
                    break;
                }
            }
        }
    }

    fn set_state(&mut self, new_state: ServerState, _ctx: &mut Context<Self>) {
        let state_clone = Arc::clone(&self.state);
        {
            let mut state = state_clone.lock().unwrap();
            *state = new_state.clone();
        }
        // notify on start and stop
        if new_state == ServerState::Running || new_state == ServerState::Stopped {
            self.notify.notify_one();
        }
    }
}

impl Actor for Mailbox {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Actix Mailbox has started");

        self.set_state(ServerState::Running, ctx);
    }
}

impl Handler<Ping> for Mailbox {
    type Result = ();
    fn handle(&mut self, _: Ping, ctx: &mut Self::Context) -> Self::Result {
        let packet_number = *self.packet_sequence_number.lock().unwrap();
        ctx.address()
            .do_send(Packet::new_start_ping_check(StartPingCheck {
                ping_id: self.ping_info.ping_number,
                oldest_unacked: packet_number,
            }));
        self.ping_info.ping_number += 1;
        self.ping_info.last_ping = time::Instant::now();
        info!("PING SENT")
    }
}

impl Handler<Pong> for Mailbox {
    type Result = ();
    fn handle(&mut self, _: Pong, ctx: &mut Self::Context) -> Self::Result {
        let packet_number = *self.packet_sequence_number.lock().unwrap();
        ctx.address()
            .do_send(Packet::new_start_ping_check(StartPingCheck {
                ping_id: self.ping_info.ping_number,
                oldest_unacked: packet_number,
            }));
        self.ping_info.ping_latency = time::Instant::now() - self.ping_info.last_ping;
        info!("PONG RECEIVED")
    }
}

impl Handler<UiMessage> for Mailbox {
    type Result = ();
    fn handle(&mut self, msg: UiMessage, _: &mut Self::Context) -> Self::Result {
        if let Some(socket) = &self.server_to_ui_socket {
            let max_message_size = 1024;
            // leave a little room at the end
            let overhead = 2;

            let message_type_len = msg.message_type.to_string().len();
            let sequence_number_len = std::mem::size_of::<u16>(); // 2 bytes for the sequence number
            let total_packet_number_len = std::mem::size_of::<u16>();
            let packet_number_len = std::mem::size_of::<u16>();

            // Calculate the maximum size available for the actual message content
            let available_size = max_message_size
                - (message_type_len
                    + sequence_number_len
                    + total_packet_number_len
                    + packet_number_len
                    + overhead);

            // Split the message content if it's larger than the available size
            let message = msg.message;
            let total_chunks = message.len().div_ceil(available_size);

            // Loop through each chunk and send it
            for chunk_index in 0..total_chunks {
                let start = chunk_index * available_size;
                let end = usize::min(start + available_size, message.len());
                let chunk = &message[start..end];

                // Increment the sequence number for each chunk
                let sequence_number = msg.sequence_number + chunk_index as u16;

                // Create a new message with the chunked data
                let chunked_message = UiMessage {
                    message_type: msg.message_type.clone(),
                    sequence_number,
                    total_packet_number: total_chunks as u16, // Add total number of chunks
                    message: chunk.to_vec(),
                    packet_number: self.sent_packet_count,
                };

                // Send the chunk using the UnixDatagram socket
                let client_socket = UnixDatagram::unbound().unwrap();
                if let Err(e) =
                    client_socket.send_to(&chunked_message.as_bytes(), &socket.socket_path)
                {
                    error!(
                        "Error sending chunk {} of {} from mailbox: {:?}",
                        sequence_number, total_chunks, e
                    )
                }
            }
            self.sent_packet_count += 1;
        }
    }
}

impl Handler<ServerToUiSocket> for Mailbox {
    type Result = ();
    fn handle(&mut self, msg: ServerToUiSocket, _: &mut Self::Context) -> Self::Result {
        self.server_to_ui_socket = Some(msg);
    }
}

// set the session to initialized.
impl Handler<Session> for Mailbox {
    type Result = ();
    fn handle(&mut self, mut msg: Session, ctx: &mut Self::Context) -> Self::Result {
        info!("SESSION IS: {:?}", msg);
        if let Some(session) = self.session.as_ref() {
            msg.socket = session.socket.clone();
        }
        self.session = Some(msg);

        // if the session doesn't already have a UDP socket to watch, create one
        if let Some(session) = self.session.as_ref() {
            if session.socket.is_none() {
                let addr = format!("127.0.0.1:{}", self.client_socket);

                let addr_clone = addr.clone();
                let mailbox_addr = ctx.address();

                info!("session established, starting UDP processing");
                let ack_queue = self.ack_queue.clone();

                let fut = async move {
                    match UdpSocket::bind(&addr).await {
                        Ok(sock) => {
                            info!("Successfully bound to {}", &addr);
                            let sock = Arc::new(sock);
                            // Spawn a new Tokio task for reading from the socket
                            tokio::spawn(Mailbox::start_udp_read(
                                ack_queue,
                                sock.clone(),
                                mailbox_addr,
                            ));
                            Ok(sock) // Return the socket wrapped in Arc
                        }
                        Err(e) => {
                            error!("Failed to bind to {}: {}", &addr_clone, e);
                            Err(e)
                        }
                    }
                };

                // wait for the socket to be successfully bound and then assign it
                ctx.spawn(fut.into_actor(self).map(|result, act, _| match result {
                    Ok(sock) => {
                        if let Some(session) = &mut act.session {
                            session.socket = Some(sock);
                        }
                    }
                    Err(_) => {
                        panic!("Socket binding failed");
                    }
                }));
            }
        }
    }
}

impl Handler<Packet> for Mailbox {
    type Result = ();
    fn handle(&mut self, mut msg: Packet, ctx: &mut Self::Context) -> Self::Result {
        if let Some(ref session) = self.session {
            let addr = format!("{}:{}", session.url, session.server_socket);
            info!("address of packet to send is {:?}", addr);
            {
                let sequence_number = self.packet_sequence_number.lock().unwrap();
                msg.header.sequence_number = *sequence_number;
                println!("PACKET NUMBER IS: {}", *sequence_number);
            }

            if msg.header.reliable {
                let ack_future = send_ack(
                    msg,
                    addr,
                    self.ack_queue.clone(),
                    session.socket.as_ref().unwrap().clone(),
                );
                ctx.spawn(
                    async move {
                        if let Err(e) = ack_future.await {
                            error!("Error sending acknowledgment: {:?}", e);
                        }
                    }
                    .into_actor(self),
                );
            } else {
                let data = msg.to_bytes().clone();
                let socket_clone = session.socket.as_ref().unwrap().clone();
                let fut = async move {
                    if let Err(e) = socket_clone.send_to(&data, &addr).await {
                        error!("Failed to send data: {}", e);
                    }
                    info!("sent data to {}", addr)
                };
                ctx.spawn(fut.into_actor(self));
            };
            {
                let mut sequence_number = self.packet_sequence_number.lock().unwrap();
                *sequence_number += 1;
            }
        }
    }
}

async fn send_ack(
    packet: Packet,
    addr: String,
    ack_queue: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
    socket: Arc<UdpSocket>,
) -> Result<(), SessionError> {
    let mut attempts = 0;
    let mut received_ack = false;
    let packet_id = packet.header.sequence_number;

    println!("PACKET IS: {:?}", packet);
    println!("SENDING ACK FOR PACKET ID: {}", packet_id);

    while attempts < ACK_ATTEMPTS && !received_ack {
        let (tx, rx) = oneshot::channel();
        let mut packet_clone = packet.clone();

        // if there have been more than 1 attempt, set the resent to true.
        if attempts > 0 {
            packet_clone.header.resent = true;
        }

        {
            let mut queue = ack_queue.lock().unwrap();
            queue.insert(packet_id, tx);

            println!("QUEUE IS: {:?}", queue);
        }
        // Send the packet

        let data = packet_clone.to_bytes().clone();
        let addr_clone = addr.clone();
        let sock_clone = socket.clone();
        if let Err(e) = sock_clone.send_to(&data, addr_clone).await {
            error!("Failed to send Ack: {}", e);
        }

        tokio::select! {
            _ = rx => {
                println!("RECEIVED ACK FOR {}", packet_id);
                received_ack = true;
            },
            _ = tokio::time::sleep(ACK_TIMEOUT) => {
                println!("Attempt {} receive acknowledgment", attempts);
                attempts += 1;
                if !received_ack && attempts >= ACK_ATTEMPTS {
                    // Remove from queue after final attempt
                    let mut queue = ack_queue.lock().unwrap();
                    queue.remove(&packet_id);
                }
            }
        }
    }
    if received_ack {
        Ok(())
    } else {
        Err(SessionError::AckError(AckError::new(
            "failed to retrieve ack ".to_string(),
        )))
    }
}
