use std::{fs, path::PathBuf};

use metaverse_messages::http::mesh::Mesh;

#[test]
pub fn test_build_skeleton() {
    let mut json_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    json_path.push("tests/data/avatar_raw_mesh_object.json");
    let json_str =
        fs::read_to_string(&json_path).unwrap_or_else(|_| panic!("Failed to read {:?}", json_path));
    let avatar_mesh: Mesh = serde_json::from_str(&json_str)
        .unwrap_or_else(|e| panic!("Failed to deserialize Mesh {:?}", e));
    println!("{:?}", avatar_mesh)
}
