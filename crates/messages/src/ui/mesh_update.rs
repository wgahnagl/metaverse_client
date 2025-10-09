use bincode::config;
use bincode::serde::{decode_from_slice, encode_to_vec};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::str::from_utf8;
use uuid::Uuid;

use crate::packet::packet::PacketData;

/// this is the struct for sending mesh updates from the core to the UI.
/// the path is the path to the generated gltf file, and the position is where to place it in the
/// world.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct MeshUpdate {
    /// path to the generated gtlf file that contains the mesh
    pub path: PathBuf,
    /// Where to render the mesh
    pub position: Vec3,
    /// The type of mesh getting rendered. Land, Object, Avatar, etc.
    pub mesh_type: MeshType,
    /// ID of the mesh. For agents, this will be the AgentID.
    pub id: Option<Uuid>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
/// Type of mesh the UI is rendering.
pub enum MeshType {
    /// Land type
    Land,
    /// Avatar type
    #[default]
    Avatar,
}

impl MeshUpdate {
    /// convert the layer update to bytes to send to the UI
    pub fn to_bytes(&self) -> Vec<u8> {
        encode_to_vec(self, config::legacy()).expect("Failed to serialize LayerUpdate")
    }
    /// convert the bytes back to a layer update struct
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        decode_from_slice(bytes, config::legacy())
            .ok()
            .map(|(mesh_update, _)| mesh_update)
    }
}

impl PacketData for MeshUpdate {
    fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        let s = from_utf8(bytes).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        serde_json::from_str(s).map_err(|e| Error::new(ErrorKind::InvalidData, e))
    }
    fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_string(&self).unwrap().into_bytes()
    }
}
