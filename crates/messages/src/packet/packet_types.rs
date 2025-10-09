use std::fmt::Debug;
use std::io;

use super::header::PacketFrequency;
use crate::agent::agent_wearables_request::AgentWearablesRequest;
use crate::agent::agent_wearables_update::AgentWearablesUpdate;
use crate::agent::avatar_appearance::AvatarAppearance;
use crate::core::object_update::ObjectUpdate;
use crate::login::logout_request::LogoutRequest;
use crate::packet::packet::PacketData;
use crate::ui::errors::SessionError;
use crate::{
    agent::{agent_update::AgentUpdate, coarse_location_update::CoarseLocationUpdate},
    chat::{chat_from_simulator::ChatFromSimulator, chat_from_viewer::ChatFromViewer},
    core::{
        complete_ping_check::CompletePingCheck, disable_simulator::DisableSimulator,
        packet_ack::PacketAck, region_handshake::RegionHandshake,
        region_handshake_reply::RegionHandshakeReply, start_ping_check::StartPingCheck,
    },
    environment::layer_data::LayerData,
    login::{
        circuit_code::CircuitCodeData, complete_agent_movement::CompleteAgentMovementData,
        login_response::LoginResponse, login_xmlrpc::Login,
    },
    ui::{mesh_update::MeshUpdate, ui_events::UiEventTypes},
};

/// Functions for determining the type of the packet.
impl PacketType {
    /// if a packet is a UI event, assign it to the corresponding UI event struct.
    pub fn ui_event(&self) -> UiEventTypes {
        match self {
            PacketType::ChatFromSimulator(_) => UiEventTypes::ChatFromSimulatorEvent,
            PacketType::CoarseLocationUpdate(_) => UiEventTypes::CoarseLocationUpdateEvent,
            PacketType::DisableSimulator(_) => UiEventTypes::DisableSimulatorEvent,
            _ => UiEventTypes::None,
        }
    }
}

/// Macro to define all PacketType variants and generate `to_bytes` and `from_id`.
macro_rules! define_packets {
    ( $( $id:literal => $variant:ident($ty:ty) [$freq:ident] ),* $(,)? ) => {
        #[derive(Debug, Clone)]
        pub enum PacketType {
            $(
                $variant(Box<$ty>),
            )*
        }

        impl PacketType {
            /// Serialize packet to bytes
            pub fn to_bytes(&self) -> Vec<u8> {
                match self {
                    $(
                        PacketType::$variant(data) => data.to_bytes(),
                    )*
                }
            }

            /// Deserialize a packet from an ID + frequency + bytes
            pub fn from_id(id: u16, frequency: PacketFrequency, bytes: &[u8]) -> io::Result<Self> {
                $(
                    if id == $id && frequency == PacketFrequency::$freq {
                        return Ok(PacketType::$variant(Box::new(PacketData::from_bytes(bytes)?)));
                    }
                )*

                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unknown packet ID: {}, frequency: {:?}", id, frequency),
                ))
            }
        }
    };
}

// Example usage:
define_packets! {
    1 => StartPingCheck(StartPingCheck) [High],
    2 => CompletePingCheck(CompletePingCheck) [High],
    4 => AgentUpdate(AgentUpdate) [High],
    11 => LayerData(LayerData) [High],
    12 => ObjectUpdate(ObjectUpdate) [High],
    6 => CoarseLocationUpdate(CoarseLocationUpdate) [Medium],
    3 => CircuitCode(CircuitCodeData) [Low],
    80 => ChatFromViewer(ChatFromViewer) [Low],
    139 => ChatFromSimulator(ChatFromSimulator) [Low],
    148 => RegionHandshake(RegionHandshake) [Low],
    149 => RegionHandshakeReply(RegionHandshakeReply) [Low],
    152 => DisableSimulator(DisableSimulator) [Low],
    158 => AvatarAppearance(AvatarAppearance) [Low],
    249 => CompleteAgentMovementData(CompleteAgentMovementData) [Low],
    252 => LogoutRequest(LogoutRequest) [Low],
    382 => AgentWearablesUpdate(AgentWearablesUpdate) [Low],
    251 => PacketAck(PacketAck) [Fixed],
    66 => Login(Login) [Fixed],
    999 => LoginResponse(LoginResponse)[Fixed],
    997 => AgentWearablesRequest(AgentWearablesRequest)[Fixed],
    996 => MeshUpdate(MeshUpdate)[Fixed],

    998 => Error(SessionError)[Fixed],
}
