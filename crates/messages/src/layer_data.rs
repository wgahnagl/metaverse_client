use std::io::{self, Cursor, Read};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::{
    header::{Header, PacketFrequency},
    packet::{Packet, PacketData},
    packet_types::PacketType,
};

impl Packet {
    pub fn new_layer_data(layer_data: LayerData) -> Self {
        Packet {
            header: Header {
                id: 11,
                reliable: false,
                resent: false,
                zerocoded: false,
                appended_acks: false,
                sequence_number: 0,
                frequency: PacketFrequency::Low,
                ack_list: None,
                size: None,
            },
            body: PacketType::LayerData(Box::new(layer_data)),
        }
    }
}

/// add your struct fields here
#[derive(Debug, Clone)]
pub struct LayerData{
    layer_id: LayerType,
    stride: u16, 
    patch_size: u8, 
    layer_type: LayerType,
    layer_content: Vec<u8>
}


#[derive(Debug, Clone)]
pub enum LayerType{
    Land,
    LandExtended,
    Water,
    WaterExtended,
    Wind,
    WindExtended,
    Cloud,
    CloudExtended,
    Unknown
}

impl LayerType{
    pub fn to_bytes(&self) -> u8 {
        match self{
            LayerType::Land => 76,
            LayerType::LandExtended => 77,
            LayerType::Water => 87,
            LayerType::WaterExtended => 88,
            LayerType::Wind => 55, 
            LayerType::WindExtended => 57, 
            LayerType::Cloud => 56, 
            LayerType::CloudExtended => 58,
            LayerType::Unknown => 0,
        }
    }
    pub fn from_bytes(bytes: u8) -> Self{
        match bytes{
            76 => LayerType::Land,
            77 => LayerType::LandExtended, 
            87 => LayerType::Water, 
            88 => LayerType::WaterExtended, 
            55 => LayerType::Wind, 
            57 => LayerType:: WindExtended,
            56 => LayerType::Cloud, 
            58 => LayerType::CloudExtended,
            _ => LayerType::Unknown
        }
    }
}

impl PacketData for LayerData {
    fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        let mut cursor = Cursor::new(bytes);
        let layer_bytes = cursor.read_u8()?;
        let layer_id = LayerType::from_bytes(layer_bytes);

        // skip these two unused bytes
        let _unused_bytes_1 = cursor.read_u16::<LittleEndian>()?;

        let stride = cursor.read_u16::<LittleEndian>()?;

        let patch_size = cursor.read_u8()?;
        let layer_type_bytes = cursor.read_u8()?;
        print!("layer_type_bytes: {:?}", layer_type_bytes);

        // this second layer type seems redundant
        let layer_type = LayerType::from_bytes(layer_type_bytes);


        let mut layer_content = Vec::new();
        cursor.read_to_end(&mut layer_content)?;

        println!("Received LayerData");

        // handle from bytes
        let data = LayerData{
            layer_id,
            stride, 
            patch_size, 
            layer_type,
            layer_content
        };
        println!("data :{:?}", data);
        Ok(data)
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // push your data into the new vector
        bytes
    }
}
