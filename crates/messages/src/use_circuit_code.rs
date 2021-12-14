use crate::models::use_circuit_code::*;
use std::io;
use tokio::net::UdpSocket;
use uuid::Uuid;

pub async fn use_circuit_code(
    sock: UdpSocket,
    session_addr: String,
    circuit_code: u32,
    session_id: Uuid,
    agent_id: Uuid,
) -> io::Result<()> {
    let packet =
        create_use_circuit_code_packet(create_use_circuit_code(circuit_code, session_id, agent_id))
            .unwrap();
    println!("{:?}, {:?}", session_addr, packet);
    sock.send_to(&packet, &session_addr).await.unwrap();
    Ok(())
}
