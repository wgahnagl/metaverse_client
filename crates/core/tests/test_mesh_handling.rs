use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use benthic_protocol::render_data::{AvatarObject, RenderObject};
use bevy::{
    DefaultPlugins,
    app::{App, Startup, Update},
    asset::{AssetMode, AssetPlugin, AssetServer, UnapprovedPathMode},
    camera::{Camera3d, visibility::Visibility},
    color::Color,
    ecs::{
        change_detection::DetectChanges,
        component::Component,
        hierarchy::ChildOf,
        name::Name,
        query::{Changed, With},
        resource::Resource,
        system::{Commands, Query, Res, ResMut},
    },
    gizmos::gizmos::Gizmos,
    gltf::GltfAssetLabel,
    light::DirectionalLight,
    math::{Dir3, Isometry3d},
    mesh::skinning::SkinnedMesh,
    prelude::PluginGroup,
    text::{TextColor, TextFont},
    transform::components::{GlobalTransform, Transform},
    ui::{
        AlignItems, BackgroundColor, FlexDirection, Interaction, JustifyContent, Node,
        PositionType, Val,
        widget::{Button, Text},
    },
    winit::WinitPlugin,
};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use bevy_world_serialization::WorldAssetRoot;
use glam::Vec3;
use metaverse_agent::{
    avatar::{Avatar, OutfitObject},
    skeleton::update_global_avatar_skeleton,
};
use metaverse_core::object_handler::create_render_object;
use metaverse_mesh::mesh::generate::generate_skinned_mesh;
use metaverse_messages::http::{mesh::Mesh, scene::SceneGroup};
use uuid::{Uuid, uuid};

const AGENT_ID: Uuid = uuid!("96ce3a85-273b-4a1a-b300-78bce703c325");

#[derive(Debug, Resource)]
struct CreationArtifacts {
    avatar_list: HashMap<Uuid, Avatar>,
    avatar_object: PathBuf,
    gltf_object: PathBuf,
}
impl CreationArtifacts {
    fn new() -> CreationArtifacts {
        CreationArtifacts {
            avatar_list: HashMap::new(),
            avatar_object: PathBuf::new(),
            gltf_object: PathBuf::new(),
        }
    }
}

fn mock_avatar_load() -> CreationArtifacts {
    let mut artifacts = CreationArtifacts::new();
    artifacts
        .avatar_list
        .insert(AGENT_ID, Avatar::new(AGENT_ID, Vec3::ZERO));
    artifacts
        .avatar_list
        .get_mut(&AGENT_ID)
        .unwrap()
        .outfit_size = 4;

    let generated_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/generated");
    if let Err(e) = fs::create_dir_all(&generated_dir) {
        panic!(
            "Failed to create generated directory {:?}: {:?}",
            generated_dir, e
        );
    }
    let generated_agent_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/generated/{}", AGENT_ID));
    if let Err(e) = fs::create_dir_all(&generated_agent_dir) {
        panic!(
            "Failed to create generated directory {:?}: {:?}",
            generated_agent_dir, e
        );
    }

    let scene_groups_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/scene_groups");

    for entry in fs::read_dir(&scene_groups_path)
        .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", scene_groups_path, e))
    {
        let entry = entry.unwrap_or_else(|e| panic!("Failed to read directory entry: {}", e));
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext != "json") {
            continue;
        }
        let file =
            fs::File::open(&path).unwrap_or_else(|e| panic!("Failed to open {:?}: {}", path, e));

        let scene_group: SceneGroup = serde_json::from_reader(file)
            .unwrap_or_else(|e| panic!("Failed to deserialize {:?}: {}", path, e));

        let mut render_objects: Vec<RenderObject> = Vec::new();

        for part in &scene_group.parts {
            println!("generating: {:?}", part.metadata.name);

            let mut mesh_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            mesh_path.push(format!("tests/data/meshes/{}.json", part.sculpt.texture));
            let file = fs::File::open(&mesh_path)
                .unwrap_or_else(|e| panic!("Failed to open {:?}: {}", mesh_path, e));

            let mut texture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            texture_path.push(format!(
                "tests/data/textures/{}.png",
                part.shape.texture.texture_id
            ));

            let mesh: Mesh = serde_json::from_reader(file)
                .unwrap_or_else(|e| panic!("Failed to deserialize {:?}: {}", mesh_path, e));
            let render_object = create_render_object(
                mesh,
                part.metadata.name.clone(),
                &texture_path,
                part.sculpt.texture,
            )
            .unwrap_or_else(|e| panic!("Render Object creation failed: {:?}", e));
            render_objects.push(render_object);
        }

        let json_name = format!(
            "{:?}_{}.json",
            scene_group.parts[0].sculpt.texture, scene_group.parts[0].metadata.name
        );
        let json_path = generated_agent_dir.join(json_name);
        match serde_json::to_string(&render_objects) {
            Ok(json) => {
                let mut file = File::create(&json_path).unwrap();
                file.write_all(json.as_bytes()).unwrap();
            }
            Err(e) => {
                panic!("Failed to write json :{}", e)
            }
        }
        add_object_to_avatar(
            &mut artifacts,
            AGENT_ID,
            OutfitObject::MeshObject(json_path),
        );
    }
    artifacts
}

fn add_object_to_avatar(artifacts: &mut CreationArtifacts, agent_id: Uuid, object: OutfitObject) {
    let generated_agent_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/generated/{}", AGENT_ID));
    if let Some(avatar) = artifacts.avatar_list.get_mut(&agent_id) {
        match &object {
            OutfitObject::MeshObject(path) => {
                let file = fs::File::open(path)
                    .unwrap_or_else(|e| panic!("Failed to open {:?}: {}", path, e));
                let parts: Vec<RenderObject> = serde_json::from_reader(file)
                    .unwrap_or_else(|e| panic!("Failed to read serde {:?}: {}", path, e));

                if let Some(skin) = &parts[0].skin {
                    update_global_avatar_skeleton(avatar, &skin.skeleton);
                }
            }
            _ => {
                panic!("No tests for non-mesh objects yet")
            }
        }
        avatar.items.push(object);
        if avatar.items.len() == avatar.outfit_size {
            avatar.fully_loaded = true;

            let json_paths: Vec<PathBuf> = avatar
                .items
                .clone()
                .into_iter()
                .filter_map(|item| {
                    if let OutfitObject::MeshObject(path) = item {
                        Some(path)
                    } else {
                        None
                    }
                })
                .collect();
            let avatar_object = AvatarObject {
                objects: json_paths,
                global_skeleton: avatar.skeleton.clone(),
                used_joints: avatar.used_joints.clone(),
            };

            let json_path =
                generated_agent_dir.join(format!("{}_avatar_object.json", avatar.agent_id));
            artifacts.avatar_object = json_path.clone();
            match serde_json::to_string(&avatar_object) {
                Ok(json) => {
                    let mut file = File::create(&json_path).unwrap();
                    file.write_all(json.as_bytes()).unwrap();
                }
                Err(e) => {
                    panic!("Failed to write json :{}", e)
                }
            }
            let glb_path = generated_agent_dir.join(format!("{:?}_high.glb", avatar.agent_id));
            artifacts.gltf_object = glb_path.clone();
            println!("{:?}", glb_path);
            println!("Generating skinned mesh");
            generate_skinned_mesh(json_path.clone(), glb_path.clone()).unwrap()
        }
    }
}

#[test]
/// this test debugs the entire pipeline from the very beginning.
/// Input data is the SceneObject and Mesh data coming directly from the server.
/// this is meant to create an easy debugging setup for each step of the process to find bugs in
/// mesh handling.
fn test_mesh_generation() {
    let _artifacts = mock_avatar_load();
}

/// this test builds the model and displays using Bevy.
#[test]
fn display_test_model() {
    let artifacts = mock_avatar_load();

    let tests_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/generated/{}", AGENT_ID));
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WinitPlugin {
                run_on_any_thread: true,
            })
            .set(AssetPlugin {
                file_path: tests_dir.into_string().unwrap(),
                mode: AssetMode::Unprocessed,
                unapproved_path_mode: UnapprovedPathMode::Allow,
                ..Default::default()
            }),
    );
    app.add_plugins(PanOrbitCameraPlugin);
    app.insert_resource(DebugVisibility {
        show_mesh: true,
        show_skeleton: true,
        show_joint_axes: false,
    });
    app.insert_resource(artifacts);
    app.add_systems(Startup, setup);
    app.add_systems(Update, handle_ui_buttons);
    app.add_systems(Update, update_mesh_visibility);
    app.add_systems(Update, draw_skeleton_debug);
    app.run();
}

#[derive(Resource, Default)]
struct DebugVisibility {
    show_mesh: bool,
    show_skeleton: bool,
    show_joint_axes: bool,
}

type ToggleButtonsQueryData<'a> = (
    &'a Interaction,
    &'a mut BackgroundColor,
    Option<&'a ToggleMeshButton>,
    Option<&'a ToggleSkeletonButton>,
    Option<&'a ToggleJointAxesButton>,
);

type ToggleButtonsQueryFilter = (Changed<Interaction>, With<Button>);

#[derive(Component)]
struct ToggleMeshButton;

#[derive(Component)]
struct ToggleSkeletonButton;

#[derive(Component)]
struct ToggleJointAxesButton;

fn handle_ui_buttons(
    mut interaction_query: Query<ToggleButtonsQueryData, ToggleButtonsQueryFilter>,
    mut debug_visibility: ResMut<DebugVisibility>,
) {
    for (interaction, mut bg_color, is_mesh, is_skeleton, is_joint_axes) in &mut interaction_query {
        if *interaction == Interaction::Pressed {
            bg_color.0 = Color::srgb(0.4, 0.4, 0.4);

            if is_mesh.is_some() {
                debug_visibility.show_mesh = !debug_visibility.show_mesh;
            }
            if is_skeleton.is_some() {
                debug_visibility.show_skeleton = !debug_visibility.show_skeleton;
            }
            if is_joint_axes.is_some() {
                debug_visibility.show_joint_axes = !debug_visibility.show_joint_axes;
            }
        } else if *interaction == Interaction::Hovered {
            bg_color.0 = Color::srgb(0.3, 0.3, 0.3);
        } else {
            bg_color.0 = Color::srgb(0.2, 0.2, 0.2);
        }
    }
}

fn update_mesh_visibility(
    debug_visibility: Res<DebugVisibility>,
    mut mesh_query: Query<&mut Visibility, With<SkinnedMesh>>,
) {
    if debug_visibility.is_changed() {
        let target_visibility = if debug_visibility.show_mesh {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        for mut visibility in &mut mesh_query {
            *visibility = target_visibility;
        }
    }
}
fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    artifacts: Res<CreationArtifacts>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 1.0, 5.0),
        PanOrbitCamera {
            focus: Vec3::new(0.0, 1.0, 0.0),
            ..Default::default()
        },
    ));

    commands.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(3.0, 5.0, 3.0).looking_at(Vec3::ZERO, Dir3::Y),
    ));

    commands.spawn((
        WorldAssetRoot(
            asset_server.load(GltfAssetLabel::Scene(0).from_asset(artifacts.gltf_object.clone())),
        ),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Name::new("Combined"),
        Visibility::Visible,
    ));

    commands
        .spawn((Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            ..Default::default()
        },))
        .with_children(|parent| {
            // Toggle Mesh Button
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(150.0),
                        height: Val::Px(40.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
                    ToggleMeshButton,
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new("Toggle Mesh"),
                        TextFont::default(),
                        TextColor(Color::WHITE),
                    ));
                });

            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(150.0),
                        height: Val::Px(40.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
                    ToggleSkeletonButton,
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new("Toggle Skeleton"),
                        TextFont::default(),
                        TextColor(Color::WHITE),
                    ));
                });
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(150.0),
                        height: Val::Px(40.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
                    ToggleJointAxesButton,
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new("Toggle Joint Axes"),
                        TextFont::default(),
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn draw_skeleton_debug(
    mut gizmos: Gizmos,
    debug_visibility: Res<DebugVisibility>,
    skinned_mesh_query: Query<&SkinnedMesh>,
    parent_query: Query<&ChildOf>,
    global_transform_query: Query<&GlobalTransform>,
) {
    if !debug_visibility.show_skeleton {
        return;
    }

    for skinned_mesh in &skinned_mesh_query {
        for &joint_entity in &skinned_mesh.joints {
            if let Ok(joint_transform) = global_transform_query.get(joint_entity) {
                let joint_pos = joint_transform.translation();

                // Joint position
                gizmos.sphere(
                    Isometry3d::from_translation(joint_pos),
                    0.02,
                    Color::srgb(0.0, 1.0, 0.0),
                );

                // Parent -> joint bone
                if let Ok(parent_component) = parent_query.get(joint_entity) {
                    let parent_entity = parent_component.parent();

                    if skinned_mesh.joints.contains(&parent_entity)
                        && let Ok(parent_transform) = global_transform_query.get(parent_entity)
                    {
                        let parent_pos = parent_transform.translation();

                        gizmos.line(parent_pos, joint_pos, Color::srgb(1.0, 1.0, 0.0));
                    }
                }

                // Joint local orientation axes
                if debug_visibility.show_joint_axes {
                    let rotation = joint_transform.rotation();
                    let axis_length = 0.1;

                    let x = rotation * Vec3::X * axis_length;
                    let y = rotation * Vec3::Y * axis_length;
                    let z = rotation * Vec3::Z * axis_length;

                    // X = red
                    gizmos.line(joint_pos, joint_pos + x, Color::srgb(1.0, 0.0, 0.0));

                    // Y = green
                    gizmos.line(joint_pos, joint_pos + y, Color::srgb(0.0, 1.0, 0.0));

                    // Z = blue
                    gizmos.line(joint_pos, joint_pos + z, Color::srgb(0.0, 0.0, 1.0));
                }
            }
        }
    }
}
