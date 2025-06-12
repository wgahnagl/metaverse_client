use super::session::{Mailbox, UiMessage};
use crate::{
    http_handler::{download_item, download_mesh, download_object},
    initialize::{create_sub_share_dir, initialize_skeleton},
};
use actix::{Addr, AsyncContext, Handler, Message, WrapFuture};
use glam::{Vec3, Vec4};
use log::error;
use metaverse_agent::{
    avatar::{Avatar, OutfitObject, RiggedObject},
    generate_gltf::generate_avatar_from_scenegroup,
};
use metaverse_messages::{
    capabilities::scene::SceneObject,
    utils::skeleton::{Joint, JointName, Skeleton, Transform},
};
use metaverse_messages::{
    ui::{
        mesh_update::{MeshType, MeshUpdate},
        ui_events::UiEventTypes,
    },
    utils::{item_metadata::ItemMetadata, object_types::ObjectType},
};
use std::{collections::HashMap, sync::Arc};
use std::{collections::HashSet, sync::Mutex};
use uuid::Uuid;

#[cfg(feature = "agent")]
#[derive(Debug, Message, Clone)]
#[rtype(result = "()")]
pub struct Agent {
    pub avatar: Avatar,
}

#[derive(Debug, Message)]
#[rtype(result = "()")]
/// Retrieve agent data from a capability url  
pub struct DownloadAgentAsset {
    /// The url of the capability url to retrieve data from
    pub url: String,
    /// The metadata of the item to download
    pub item: ItemMetadata,
    /// The agent ID of the avatar
    pub agent_id: Uuid,
    /// The location of the agent in space
    pub position: Vec3,
}

impl Handler<DownloadAgentAsset> for Mailbox {
    type Result = ();
    fn handle(&mut self, msg: DownloadAgentAsset, ctx: &mut Self::Context) -> Self::Result {
        if let Some(session) = self.session.as_mut() {
            let agent_list = session.agent_list.clone();
            let address = ctx.address().clone();
            ctx.spawn(
                async move {
                    match msg.item.item_type {
                        ObjectType::Object => match download_object(msg.item, &msg.url).await {
                            Ok(mut scene_group) => {
                                for scene in &mut scene_group.parts {
                                    let mut mesh_metadata = scene.item_metadata.clone();
                                    mesh_metadata.item_type = ObjectType::Mesh;
                                    mesh_metadata.asset_id = scene.sculpt.texture;
                                    match download_mesh(mesh_metadata, &msg.url).await {
                                        Ok(mesh) => {
                                            scene.sculpt.mesh = Some(mesh);
                                        }
                                        Err(e) => {
                                            error!("{:?}", e);
                                        }
                                    };
                                }
                                let skeleton = create_skeleton(
                                    scene_group.parts[0].clone(),
                                    agent_list.clone(),
                                    msg.agent_id,
                                )
                                .unwrap();
                                add_item_to_agent_list(
                                    agent_list,
                                    msg.agent_id,
                                    OutfitObject::RiggedObject(RiggedObject {
                                        scene_group,
                                        skeleton,
                                    }),
                                    address,
                                );
                            }
                            Err(e) => {
                                error!("{:?}", e);
                            }
                        },
                        ObjectType::Link => {}
                        _ => match download_item(msg.item, &msg.url).await {
                            Ok(item) => {
                                add_item_to_agent_list(
                                    agent_list,
                                    msg.agent_id,
                                    OutfitObject::Item(item),
                                    address,
                                );
                            }
                            Err(e) => {
                                error!("{:?}", e);
                            }
                        },
                    }
                }
                .into_actor(self),
            );
        }
    }
}

fn create_skeleton(
    scene_root: SceneObject,
    agent_list: Arc<Mutex<HashMap<Uuid, Avatar>>>,
    agent_id: Uuid,
) -> Option<Skeleton> {
    if let Some(mesh) = &scene_root.sculpt.mesh {
        let mut joints = HashMap::new();
        let valid_names: HashSet<_> = mesh.skin.joint_names.iter().cloned().collect();

        for (i, name) in mesh.skin.joint_names.iter().enumerate() {
            if let Some(agent) = agent_list.lock().unwrap().get_mut(&agent_id) {
                let default_joints = agent.skeleton.joints.get(name).unwrap().clone();

                // apply the rotations from the default skeleton to the object
                let mut default_transform = default_joints.transforms[0].transform.clone();
                default_transform.w_axis = Vec4::new(0.0, 0.0, 0.0, 1.0);
                let transform_matrix = default_transform * mesh.skin.inverse_bind_matrices[i];

                let transform = Transform {
                    name: scene_root.name.clone(),
                    id: scene_root.sculpt.texture,
                    transform: transform_matrix,
                };

                // create the joint object that contians the calculted transforms
                let joint = Joint {
                    name: name.clone(),
                    parent: default_joints.parent.filter(|p| valid_names.contains(p)),
                    children: default_joints
                        .children
                        .into_iter()
                        .filter(|p| valid_names.contains(p))
                        .collect(),
                    transforms: vec![transform.clone()],
                    local_transforms: vec![],
                };
                joints.insert(*name, joint);

                // update the global skeleton with the transforms
                if let Some(joint) = agent.skeleton.joints.get_mut(name) {
                    // Ignore transforms that are already applied
                    let already_exists = joint
                        .transforms
                        .iter()
                        .any(|t| t.transform.abs_diff_eq(transform.transform, 1e-4));
                    if !already_exists {
                        joint.transforms.push(transform);
                    }
                }
            };
        }
        // create a skeleton that contains the root nodes
        let root_joints: Vec<JointName> = joints
            .values()
            .filter(|joint| joint.parent.is_none())
            .map(|joint| joint.name.clone())
            .collect();

        let parent_transforms_map: HashMap<_, _> = joints
            .iter()
            .map(|(name, joint)| (name.clone(), joint.transforms.clone()))
            .collect();

        for joint in joints.values_mut() {
            if let Some(parent) = &joint.parent {
                if let Some(parent_transforms) = parent_transforms_map.get(parent) {
                    for (i, parent_transform) in parent_transforms.iter().enumerate() {
                        let mut base_transform = joint.transforms[i].clone();
                        base_transform.transform =
                            base_transform.transform * parent_transform.transform.inverse();
                        joint.local_transforms.push(base_transform);
                    }
                }
            } else {
                for transform in &joint.transforms {
                    // clone instead of move
                    joint.local_transforms.push(transform.clone());
                }
            }
        }
        Some(Skeleton {
            root: root_joints,
            joints,
        })
    } else {
        None
    }
}

fn add_item_to_agent_list(
    agent_list: Arc<Mutex<HashMap<Uuid, Avatar>>>,
    agent_id: Uuid,
    item: OutfitObject,
    address: Addr<Mailbox>,
) {
    if let Some(agent) = agent_list.lock().unwrap().get_mut(&agent_id) {
        agent.outfit_items.push(item);
        // if all of the items have loaded in, trigger a render
        if agent.outfit_items.len() == agent.outfit_size {
            address.do_send(Agent {
                avatar: agent.clone(),
            });
        }
    }
}

impl Handler<Agent> for Mailbox {
    type Result = ();
    fn handle(&mut self, msg: Agent, ctx: &mut Self::Context) -> Self::Result {
        for item in msg.avatar.outfit_items {
            match item {
                OutfitObject::RiggedObject(object) => {
                    if let Ok(agent_dir) = create_sub_share_dir("agent") {
                        if let Ok(skeleton_path) = initialize_skeleton() {
                            match generate_avatar_from_scenegroup(
                                object.scene_group,
                                object.skeleton,
                                skeleton_path,
                                agent_dir,
                            ) {
                                Ok(path) => {
                                    ctx.address().do_send(UiMessage::new(
                                        UiEventTypes::MeshUpdate,
                                        MeshUpdate {
                                            position: msg.avatar.position,
                                            path,
                                            mesh_type: MeshType::Avatar,
                                            id: Some(msg.avatar.agent_id),
                                        }
                                        .to_bytes(),
                                    ));
                                }
                                Err(e) => {
                                    error!("uh oh {:?}", e)
                                }
                            }
                        }
                    }
                }
                OutfitObject::Item(item) => {
                    println!("{:?}", item.metadata.name);
                }
            }
        }
    }
}
