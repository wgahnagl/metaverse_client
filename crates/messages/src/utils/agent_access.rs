use serde::{Deserialize, Serialize};
use xmlrpc_benthic::{self as xmlrpc, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AgentAccess {
    Adult,
    Mature,
    Down,
    NonExistent,
    Trial,
    General,
    PG,
    Unknown,
}
impl AgentAccess {
    pub fn to_bytes(&self) -> u8 {
        match self {
            AgentAccess::General => 2,
            AgentAccess::Trial => 7,
            AgentAccess::PG => 13,
            AgentAccess::Mature => 21,
            AgentAccess::Adult => 42,
            AgentAccess::Down => 254,
            AgentAccess::NonExistent => 255,
            _ => 0,
        }
    }
    pub fn from_bytes(bytes: &u8) -> Self {
        match bytes {
            2 => AgentAccess::General,
            7 => AgentAccess::Trial,
            13 => AgentAccess::PG,
            21 => AgentAccess::Mature,
            42 => AgentAccess::Adult,
            254 => AgentAccess::Down,
            255 => AgentAccess::NonExistent,
            _ => AgentAccess::Unknown,
        }
    }
}
impl From<AgentAccess> for Value {
    fn from(val: AgentAccess) -> Self {
        let access_str = match val {
            AgentAccess::Down => "Down",
            AgentAccess::NonExistent => "",
            AgentAccess::Trial => "T",
            AgentAccess::Mature => "M",
            AgentAccess::Adult => "A",
            AgentAccess::PG => "PG",
            AgentAccess::General => "G",
            AgentAccess::Unknown => "Unknown",
        };
        Value::String(access_str.to_string())
    }
}
pub fn parse_agent_access(agent_access: Option<&xmlrpc::Value>) -> Option<AgentAccess> {
    agent_access.map(|x| match x.clone().as_str().unwrap() {
        "M" => AgentAccess::Mature,
        "A" => AgentAccess::Adult,
        "PG" => AgentAccess::PG,
        "G" => AgentAccess::General,
        "" => AgentAccess::NonExistent,
        "Down" => AgentAccess::Down,
        "T" => AgentAccess::Trial,
        _ => AgentAccess::Unknown,
    })
}
