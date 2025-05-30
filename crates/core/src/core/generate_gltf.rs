use glam::Vec3;
use glam::usize;
use gltf_json::Buffer;
use gltf_json::Index;
use gltf_json::Node;
use gltf_json::Root;
use gltf_json::accessor::ComponentType;
use gltf_json::accessor::GenericComponentType;
use gltf_json::validation::Checked;
use gltf_json::validation::Checked::Valid;
use log::info;
use metaverse_messages::capabilities::scene::SceneGroup;
use metaverse_messages::ui::mesh_update::{MeshType, MeshUpdate};
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::{borrow::Cow, mem};
use uuid::Uuid;

use gltf_json::validation::USize64;
use metaverse_messages::capabilities::mesh::{Mesh, MeshGeometry};

use crate::initialize::initialize_share_dir;

/// Generate one mesh at the highest level of detail. This is the default level of detail unless
/// specified.
pub fn generate_high_lod(
    mesh: &Mesh,
    path: PathBuf,
    name: String,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = path.join(format!("{}_high.gltf", name));

    let mut root = gltf_json::Root::default();
    let buffer = root.push(gltf_json::Buffer {
        byte_length: USize64::from(0 as usize),
        extensions: Default::default(),
        extras: Default::default(),
        name: Some(name),
        uri: None,
    });
    let node = generate_node(
        &mesh.high_level_of_detail,
        USize64::from(0 as usize),
        buffer,
        None,
        None,
        &mut root,
    )?;
    root.push(gltf_json::Scene {
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        nodes: vec![node],
    });

    let buffer_length = mesh.high_level_of_detail.triangles.len() * mem::size_of::<Vec3>();
    let json_string = gltf_json::serialize::to_string(&root)?;
    let mut json_offset = json_string.len();
    align_to_multiple_of_four(&mut json_offset);
    let glb = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: (json_offset + buffer_length).try_into()?,
        },
        bin: Some(Cow::Owned(to_padded_byte_vector(
            &mesh.high_level_of_detail.triangles,
        ))),
        json: Cow::Owned(json_string.into_bytes()),
    };

    let writer = File::create(&path)?;
    glb.to_writer(writer)?;

    Ok(path)
}

fn add_node_recursive(
    document: &gltf::Document,
    root: &mut gltf_json::Root,
    joint_index_map: &mut HashMap<usize, usize>,
    node_index: usize,
) -> usize {
    if let Some(&existing_index) = joint_index_map.get(&node_index) {
        return existing_index;
    }
    let node = document
        .nodes()
        .nth(node_index)
        .expect("Node index out of range");

    let children_indices: Vec<_> = node
        .children()
        .map(|child| add_node_recursive(document, root, joint_index_map, child.index()))
        .collect();

    let (translation, rotation, scale) = node.transform().decomposed();

    let new_index = root.nodes.len();
    root.nodes.push(gltf_json::Node {
        camera: None,
        children: if children_indices.is_empty() {
            None
        } else {
            Some(children_indices.into_iter().map(|i| Index::new(i as u32)).collect())
        },
        skin: None,
        matrix: None,
        mesh: None,
        name: node.name().map(|s| s.to_string()),
        rotation: Some(gltf_json::scene::UnitQuaternion(rotation)),
        scale: Some([scale[0], scale[1], scale[2]]),
        translation: Some([translation[0], translation[1], translation[2]]),
        weights: None,
        extensions: Default::default(),
        extras: Default::default(),
    });
    joint_index_map.insert(node_index, new_index);
    new_index
}
pub fn generate_avatar_from_scenegroup(
    scene_group: SceneGroup,
    position: Vec3,
    path: PathBuf,
    agent_id: Uuid,
) -> Result<MeshUpdate, Box<dyn std::error::Error>> {
    let mut root = gltf_json::Root::default();
    let mut all_vertices = Vec::new();
    let mut nodes = Vec::new();
    let mut offset = 0;
    let high_path = path.join(format!("{}_high.gltf", scene_group.parts[0].name));

    let buffer = root.push(gltf_json::Buffer {
        byte_length: USize64::from(0 as usize),
        extensions: Default::default(),
        extras: Default::default(),
        name: Some(scene_group.parts[0].name.clone()),
        uri: None,
    });

    for scene in &scene_group.parts {
        if let Some(mesh) = scene.sculpt.mesh.as_ref() {
            let node = generate_node(
                &mesh.high_level_of_detail,
                USize64::from(offset as usize),
                buffer,
                Some([
                    scene.offset_position.x,
                    scene.offset_position.z,
                    scene.offset_position.y,
                ]),
                Some([
                    scene.object_update.scale.x,
                    scene.object_update.scale.z,
                    scene.object_update.scale.y,
                ]),
                &mut root,
            )?;
            nodes.push(node);
            let buffer_length = mesh.high_level_of_detail.triangles.len() * mem::size_of::<Vec3>();
            offset += buffer_length;
            all_vertices.extend_from_slice(&mesh.high_level_of_detail.triangles);
        }
    }

    let mut share_path = initialize_share_dir()?;
    share_path.push("skeleton.gltf");
    info!("Loading skeleton data from {:?}", share_path);
    let (document, buffers, _) = gltf::import(share_path).expect("Failed to load skeleton");

    let skin = document.skins().next().expect("No skins in gltf");
    let mut joint_index_map = HashMap::new();

    let joints: Vec<_> = skin.joints().map(|j| j.index()).collect();
    for joint in joints {
        add_node_recursive(&document, &mut root, &mut joint_index_map, joint);
    }

    let ibm_accessor = skin
        .inverse_bind_matrices()
        .expect("No inverse bind matrices in skin");
    let ibm_view = ibm_accessor
        .view()
        .expect("Inverse bind matrices must have buffer view");
    let ibm_buffer = &buffers[ibm_view.buffer().index()];

    let ibm_start = ibm_view.offset() + ibm_accessor.offset();
    let ibm_length = ibm_accessor.count() * 16 * mem::size_of::<f32>();

    let ibm_bytes = &ibm_buffer[ibm_start..(ibm_start + ibm_length)];

    let mut ibm_floats = Vec::with_capacity(ibm_accessor.count() * 16);
    for chunk in ibm_bytes.chunks_exact(4) {
        let arr: [u8; 4] = chunk.try_into().expect("Chunk size must be 4");
        ibm_floats.push(f32::from_le_bytes(arr));
    }

    let ibm_buffer_view = gltf_json::buffer::View {
        buffer,
        byte_length: USize64::from(ibm_bytes.len()),
        byte_offset: Some(USize64::from(all_vertices.len() * mem::size_of::<Vec3>())),
        byte_stride: None,
        target: None,
        name: Some("inverse_bind_matrices_view".to_string()),
        extensions: Default::default(),
        extras: Default::default(),
    };
    let ibm_buffer_view_index = root.push(ibm_buffer_view);

    let ibm_bytes_vec: Vec<u8> = ibm_floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    all_vertices.extend_from_slice(unsafe {
        std::slice::from_raw_parts(
            ibm_bytes_vec.as_ptr() as *const Vec3,
            ibm_bytes_vec.len() / mem::size_of::<Vec3>(),
        )
    });

    let ibm_accessor_json = gltf_json::Accessor {
        sparse: None,
        buffer_view: Some(ibm_buffer_view_index),
        byte_offset: Some(USize64::from(0 as usize)),
        count: USize64::from(ibm_accessor.count() as usize),
        component_type: Checked::Valid(gltf_json::accessor::GenericComponentType(
            ComponentType::F32,
        )),
        extensions: Default::default(),
        extras: Default::default(),
        max: None,
        min: None,
        name: Some("inverse_bind_matrices_accessor".to_string()),
        normalized: false,
        type_: Checked::Valid(gltf_json::accessor::Type::Mat4),
    };

    let inverse_bind_accessor_index = root.push(ibm_accessor_json);

    let joints: Vec<_> = skin
        .joints()
        .map(|j| Index::new(joint_index_map[&j.index()] as u32))
        .collect();

    root.skins.push(gltf_json::Skin {
        joints,
        inverse_bind_matrices: Some(inverse_bind_accessor_index),
        skeleton: skin
            .skeleton()
            .map(|n| Index::new(joint_index_map[&n.index()] as u32)),
        extensions: Default::default(),
        extras: Default::default(),
        name: skin.name().map(str::to_string),
    });

    root.push(gltf_json::Scene {
        extensions: Default::default(),
        extras: Default::default(),
        name: Some(scene_group.parts[0].name.clone()),
        nodes,
    });







    let buffer_length = all_vertices.len() * mem::size_of::<Vec3>();
    root.buffers[buffer.value()] = gltf_json::Buffer {
        byte_length: USize64::from(buffer_length),
        ..root.buffers[buffer.value()].clone()
    };

    let json_string = gltf_json::serialize::to_string(&root)?;
    let mut json_offset = json_string.len();
    align_to_multiple_of_four(&mut json_offset);

    let glb = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: (json_offset + buffer_length).try_into()?,
        },
        json: Cow::Owned(json_string.into_bytes()),
        bin: Some(Cow::Owned(to_padded_byte_vector(&all_vertices))),
    };

    let writer = File::create(&high_path)?;
    glb.to_writer(writer)?;



    Ok(MeshUpdate {
        position,
        path: high_path,
        mesh_type: MeshType::Avatar,
        id: Some(agent_id),
    })
}

/// Generates the mesh for land layers from the heightmap.
/// exports as gltf files in the share dir, labeled `x_y_<hash>.glb`
///
/// heavily referenced from
/// <https://github.com/gltf-rs/gltf/blob/main/examples/export/main.rs>
pub fn generate_node(
    data: &MeshGeometry,
    offset: USize64,
    buffer: Index<Buffer>,
    translation: Option<[f32; 3]>,
    scale: Option<[f32; 3]>,
    root: &mut Root,
) -> Result<Index<Node>, Box<dyn std::error::Error>> {
    let buffer_length = data.triangles.len() * mem::size_of::<Vec3>();
    let (min, max) = bounding_coords(&data.triangles);
    let buffer_view = root.push(gltf_json::buffer::View {
        buffer,
        byte_length: USize64::from(buffer_length),
        byte_offset: Some(offset),
        byte_stride: Some(gltf_json::buffer::Stride(mem::size_of::<Vec3>())),
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        target: Some(Valid(gltf_json::buffer::Target::ArrayBuffer)),
    });
    let positions = root.push(gltf_json::Accessor {
        buffer_view: Some(buffer_view),
        byte_offset: Some(USize64(0)),
        count: USize64::from(data.triangles.len()),
        component_type: Valid(gltf_json::accessor::GenericComponentType(
            gltf_json::accessor::ComponentType::F32,
        )),
        extensions: Default::default(),
        extras: Default::default(),
        type_: Valid(gltf_json::accessor::Type::Vec3),
        min: Some(gltf_json::Value::from(Vec::from(min))),
        max: Some(gltf_json::Value::from(Vec::from(max))),
        name: None,
        normalized: false,
        sparse: None,
    });
    let primitive = gltf_json::mesh::Primitive {
        attributes: {
            let mut map = std::collections::BTreeMap::new();
            map.insert(Valid(gltf_json::mesh::Semantic::Positions), positions);
            map
        },
        extensions: Default::default(),
        extras: Default::default(),
        indices: None,
        material: None,
        mode: Valid(gltf_json::mesh::Mode::Triangles),
        targets: None,
    };
    let mesh = root.push(gltf_json::Mesh {
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        primitives: vec![primitive],
        weights: None,
    });
    let node = root.push(gltf_json::Node {
        mesh: Some(mesh),
        translation,
        scale,
        ..Default::default()
    });
    Ok(node)
}

/// realigns the data to a mutiple of four
fn align_to_multiple_of_four(n: &mut usize) {
    *n = (*n + 3) & !3;
}

/// Converts a byte vector to a vector aligned to a mutiple of 4
fn to_padded_byte_vector(data: &[Vec3]) -> Vec<u8> {
    let flat: Vec<[f32; 3]> = data.iter().map(|v| [v.x, v.y, v.z]).collect();
    let byte_slice: &[u8] = bytemuck::cast_slice(&flat);
    let mut new_vec: Vec<u8> = byte_slice.to_owned();

    while new_vec.len() % 4 != 0 {
        new_vec.push(0); // pad to multiple of four bytes
    }

    new_vec
}

/// determines the highest and lowest points on the mesh to store as min and max
///fn bounding_coords(points: &[Vec3]) -> ([f32; 3], [f32; 3]) {
fn bounding_coords(points: &[Vec3]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX, f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN, f32::MIN];

    for p in points {
        for i in 0..3 {
            min[i] = f32::min(min[i], p[i]);
            max[i] = f32::max(max[i], p[i]);
        }
    }
    (min, max)
}
