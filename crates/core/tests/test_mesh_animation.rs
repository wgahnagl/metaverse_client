use benthic_protocol::default_animations::DefaultAnimation;
use benthic_protocol::skeleton::{JointName, Skeleton, SkinJoint};
use bevy::app::Update;
use bevy::asset::{AssetMode, AssetPlugin, UnapprovedPathMode};
use bevy::camera::Camera3d;
use bevy::color::Color;
use bevy::ecs::prelude::*;
use bevy::light::PointLight;
use bevy::math::primitives::Sphere;
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use bevy::utils::default;
use bevy::winit::WinitPlugin;
use bevy::{
    DefaultPlugins,
    animation::AnimationPlayer,
    app::{App, PluginGroup, Startup},
    asset::{AssetServer, Assets, Handle},
    ecs::system::{Commands, Query, Res, ResMut},
    gltf::GltfAssetLabel,
    prelude::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex, Resource},
};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use glam::Vec3;
use lazy_static::lazy_static;
use metaverse_avatar::animation::build_animation;
use metaverse_avatar::errors::AnimationError;
use metaverse_messages::udp::agent::avatar_animation::AnimationEntry;
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::common::bevy::{
    DebugVisibility, draw_skeleton_debug, handle_ui_buttons, setup, update_mesh_visibility,
};
use crate::common::{AGENT_ID, mock_avatar_load};
mod common;

lazy_static! {
    static ref PUFFBALL_JOINT_FILTER: BTreeSet<SkinJoint> = BTreeSet::from([
        SkinJoint::Joint(JointName::Pelvis),
        SkinJoint::Joint(JointName::Torso),
        SkinJoint::Joint(JointName::Tail1),
        SkinJoint::Joint(JointName::HipLeft),
        SkinJoint::Joint(JointName::HipRight),
        SkinJoint::Joint(JointName::Chest),
        SkinJoint::Joint(JointName::Neck),
        SkinJoint::Joint(JointName::CollarLeft),
        SkinJoint::Joint(JointName::CollarRight),
        SkinJoint::Joint(JointName::Head),
        SkinJoint::Joint(JointName::Skull),
        SkinJoint::Joint(JointName::FaceRoot),
        SkinJoint::Joint(JointName::FaceJaw),
        SkinJoint::Joint(JointName::FaceJawShaper),
        SkinJoint::Joint(JointName::FaceEar1Left),
        SkinJoint::Joint(JointName::FaceEar1Right),
        SkinJoint::Joint(JointName::FaceEar2Left),
        SkinJoint::Joint(JointName::FaceEar2Right),
        SkinJoint::Joint(JointName::ShoulderLeft),
        SkinJoint::Joint(JointName::ElbowLeft),
        SkinJoint::Joint(JointName::WristLeft),
        SkinJoint::Joint(JointName::HandIndex1Left),
        SkinJoint::Joint(JointName::HandMiddle1Left),
        SkinJoint::Joint(JointName::HandRing1Left),
        SkinJoint::Joint(JointName::HandPinky1Left),
        SkinJoint::Joint(JointName::HandThumb1Left),
        SkinJoint::Joint(JointName::HandIndex2Left),
        SkinJoint::Joint(JointName::HandMiddle2Left),
        SkinJoint::Joint(JointName::HandRing2Left),
        SkinJoint::Joint(JointName::HandPinky2Left),
        SkinJoint::Joint(JointName::HandThumb2Left),
        SkinJoint::Joint(JointName::HandIndex3Left),
        SkinJoint::Joint(JointName::HandMiddle3Left),
        SkinJoint::Joint(JointName::HandRing3Left),
        SkinJoint::Joint(JointName::HandPinky3Left),
        SkinJoint::Joint(JointName::HandThumb3Left),
        SkinJoint::Joint(JointName::ShoulderRight),
        SkinJoint::Joint(JointName::ElbowRight),
        SkinJoint::Joint(JointName::WristRight),
        SkinJoint::Joint(JointName::HandIndex1Right),
        SkinJoint::Joint(JointName::HandMiddle1Right),
        SkinJoint::Joint(JointName::HandRing1Right),
        SkinJoint::Joint(JointName::HandPinky1Right),
        SkinJoint::Joint(JointName::HandThumb1Right),
        SkinJoint::Joint(JointName::HandIndex2Right),
        SkinJoint::Joint(JointName::HandMiddle2Right),
        SkinJoint::Joint(JointName::HandRing2Right),
        SkinJoint::Joint(JointName::HandPinky2Right),
        SkinJoint::Joint(JointName::HandThumb2Right),
        SkinJoint::Joint(JointName::HandIndex3Right),
        SkinJoint::Joint(JointName::HandMiddle3Right),
        SkinJoint::Joint(JointName::HandRing3Right),
        SkinJoint::Joint(JointName::HandPinky3Right),
        SkinJoint::Joint(JointName::HandThumb3Right),
        SkinJoint::Joint(JointName::Tail2),
        SkinJoint::Joint(JointName::Tail3),
        SkinJoint::Joint(JointName::Tail4),
        SkinJoint::Joint(JointName::Tail5),
        SkinJoint::Joint(JointName::Tail6),
        SkinJoint::Joint(JointName::KneeLeft),
        SkinJoint::Joint(JointName::AnkleLeft),
        SkinJoint::Joint(JointName::FootLeft),
        SkinJoint::Joint(JointName::KneeRight),
        SkinJoint::Joint(JointName::AnkleRight),
        SkinJoint::Joint(JointName::FootRight),
    ]);
}

fn generated_animation_path(name: &str) -> PathBuf {
    let path = PathBuf::from("tests")
        .join("generated")
        .join(AGENT_ID.to_string())
        .join("animation");

    let final_path = path.join(name);
    std::fs::create_dir_all(&final_path).unwrap();
    final_path
}

async fn load_animation(
    name: &str,
    target_skeleton: &Skeleton,
    used_joints: &BTreeSet<SkinJoint>,
) -> Result<Vec<PathBuf>, AnimationError> {
    let anim_id = DefaultAnimation::from_string(name)
        .unwrap_or_else(|| panic!("unknown default animation: {name}"))
        .uuid();

    let animations = vec![AnimationEntry {
        anim_id,
        sequence_id: 1,
    }];

    build_animation(
        animations,
        used_joints.clone(),
        target_skeleton.clone(),
        generated_animation_path("Retargeted"),
        AGENT_ID,
    )
    .await
}

#[tokio::test]
async fn load_stand() {
    let artifacts = mock_avatar_load().await;
    load_animation(
        "Stand",
        &artifacts.avatar_object.global_skeleton,
        &artifacts.avatar_object.used_joints,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn run_stand() {
    display_animation("Stand").await;
}

#[tokio::test]
async fn run_dance() {
    display_animation("Dance").await;
}

#[tokio::test]
async fn build_test_animation() {
    let artifacts = mock_avatar_load().await;

    let animations = vec![AnimationEntry {
        anim_id: DefaultAnimation::Stand.uuid(),
        sequence_id: 1,
    }];

    let paths = build_animation(
        animations,
        artifacts.avatar_object.used_joints.clone(),
        artifacts.avatar_object.global_skeleton.clone(),
        generated_animation_path("Retargeted"),
        AGENT_ID,
    )
    .await
    .unwrap();

    println!("Generated animations: {:?}", paths);
}

fn display_vertices(vertices_a: Vec<Vec3>, vertices_b: Vec<Vec3>) {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(WinitPlugin {
        run_on_any_thread: true,
    }));
    app.add_plugins(PanOrbitCameraPlugin);

    app.add_systems(
        Startup,
        move |mut commands: Commands,
              mut meshes: ResMut<Assets<Mesh>>,
              mut materials: ResMut<Assets<StandardMaterial>>| {
            let mesh = meshes.add(Sphere::new(0.015).mesh().ico(2).unwrap());

            // material a is red
            let material_a = materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.0, 0.0),
                ..default()
            });

            // material b is green
            let material_b = materials.add(StandardMaterial {
                base_color: Color::srgb(0.0, 1.0, 0.0),
                ..default()
            });

            for position in &vertices_a {
                commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material_a.clone()),
                    Transform::from_translation(*position),
                ));
            }

            for position in &vertices_b {
                commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material_b.clone()),
                    Transform::from_translation(*position),
                ));
            }

            commands.spawn((
                Camera3d::default(),
                Transform::from_xyz(0.0, -3.0, 1.5).looking_at(Vec3::new(0.0, 0.0, 0.8), Vec3::Z),
                PanOrbitCamera {
                    focus: Vec3::new(0.0, 0.0, 0.8),
                    radius: Some(3.0),
                    ..default()
                },
            ));

            commands.spawn((
                PointLight {
                    intensity: 5000.0,
                    ..default()
                },
                Transform::from_xyz(2.0, 2.0, 4.0),
            ));
        },
    );

    app.run();
}
#[derive(Debug, Resource)]
struct AnimationName(String);

async fn display_animation(animation: &str) {
    let artifacts = mock_avatar_load().await;

    let animation_path = load_animation(
        animation,
        &artifacts.avatar_object.global_skeleton,
        &artifacts.avatar_object.used_joints,
    )
    .await
    .unwrap()
    .last()
    .unwrap()
    .clone();

    let animation_dir = animation_path.parent().unwrap().to_path_buf();
    let animation_file = animation_path.file_name().unwrap().to_string_lossy();

    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(WinitPlugin {
                run_on_any_thread: true,
            })
            .set(AssetPlugin {
                file_path: animation_dir.to_string_lossy().into_owned(),
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
    app.insert_resource(AnimationName(animation_file.to_string()));

    app.add_systems(Startup, setup);
    app.add_systems(Update, handle_ui_buttons);
    app.add_systems(Update, update_mesh_visibility);
    app.add_systems(Update, draw_skeleton_debug);
    app.add_systems(Startup, setup_animation_graph);

    app.add_observer(animation_player_added);

    app.run();
}

fn setup_animation_graph(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    animation_name: Res<AnimationName>,
) {
    let mut graph = AnimationGraph::new();

    let animation_path = format!("{}", animation_name.0);

    let animations = vec![graph.add_clip(
        asset_server.load(GltfAssetLabel::Animation(0).from_asset(animation_path)),
        1.0,
        graph.root,
    )];

    let graph_handle = graphs.add(graph);

    commands.insert_resource(AnimationGraphCache {
        animations,
        graph: graph_handle,
    });
}

#[derive(Debug, Resource)]
struct AnimationGraphCache {
    animations: Vec<AnimationNodeIndex>,
    graph: Handle<AnimationGraph>,
}

fn animation_player_added(
    trigger: On<Add, AnimationPlayer>,
    mut commands: Commands,
    graph_cache: Res<AnimationGraphCache>,
    mut players: Query<&mut AnimationPlayer>,
) {
    if let Ok(mut player) = players.get_mut(trigger.entity) {
        player.play(graph_cache.animations[0]).repeat();

        commands
            .entity(trigger.entity)
            .insert(AnimationGraphHandle(graph_cache.graph.clone()));
    }
}
