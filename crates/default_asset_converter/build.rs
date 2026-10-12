use benthic_protocol::default_animations::{
    AnimationClip, DefaultAnimation, JointAnimation, Keyframe,
};
use benthic_protocol::skeleton::JointName::Pelvis;
use benthic_protocol::skeleton::{CollisionName, Joint, JointName, Skeleton, SkinJoint, Transform};
use bvh_anim::ChannelType;
use glam::Mat4;
use glam::{Quat, Vec3};
use gltf::Node;
use indexmap::IndexMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::process::Command;
use std::{collections::HashMap, env, fs, path::PathBuf, str::FromStr};
use uuid::Uuid;

const DEFAULT_ASSETS_REPO: &str = "https://github.com/benthic-mmo/benthic_default_assets.git";
const DEFAULT_ASSETS_TAG: &str = "v0.3.1";

fn download_default_assets(out_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let assets_dir = out_dir.join("benthic_default_assets");

    if assets_dir.exists() {
        return Ok(assets_dir);
    }

    println!("Downloading benthic_default_assets {DEFAULT_ASSETS_TAG}");

    let status = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--branch",
            DEFAULT_ASSETS_TAG,
            DEFAULT_ASSETS_REPO,
        ])
        .arg(&assets_dir)
        .status()
        .map_err(|e| format!("Failed to execute git: {e}"))?;

    if !status.success() {
        return Err(
            format!("Failed to download benthic_default_assets {DEFAULT_ASSETS_TAG}").into(),
        );
    }

    Ok(assets_dir)
}

fn get_default_assets(out_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if std::env::var_os("CARGO_FEATURE_LOCAL_ASSETS").is_some() {
        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);

        let assets_dir = std::env::var_os("BENTHIC_DEFAULT_ASSETS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| manifest_dir.join("../../../benthic_default_assets/"));

        if !assets_dir.join("Textures/default.png").is_file()
            || !assets_dir.join("Skeleton/skeleton.gltf").is_file()
        {
            return Err(format!(
                "Local default assets not found or incomplete: {}",
                assets_dir.display()
            )
            .into());
        }

        println!("cargo:rerun-if-env-changed=BENTHIC_DEFAULT_ASSETS_PATH");
        println!("Using local default assets: {}", assets_dir.display());

        Ok(assets_dir)
    } else {
        download_default_assets(out_dir)
    }
}

fn main() {
    let gen_animations = std::env::var("CARGO_FEATURE_ANIMATIONS").is_ok();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let default_assets = match get_default_assets(&out_dir) {
        Ok(path) => path,
        Err(error) => {
            println!(
                "cargo:warning=FAILED TO FETCH DEFAULT ASSETS. USING MOCK ASSETS: {}",
                error
            );
            let skeleton_file = out_dir.join("default_skeleton.json");
            let skeleton = Skeleton::default();

            let json = serde_json::to_string_pretty(&skeleton)
                .expect("Failed to serialize default skeleton");

            fs::write(&skeleton_file, json).expect("Failed to write fallback default skeleton");

            println!("cargo:rustc-env=BENTHIC_DEFAULT_ASSETS=");
            return;
        }
    };

    println!(
        "cargo:rustc-env=BENTHIC_DEFAULT_ASSETS={}",
        default_assets.display()
    );

    // copy the default texture to the build dir

    let texture_path = default_assets.join("Textures");
    fs::copy(
        texture_path.join("default.png"),
        out_dir.join("default.png"),
    )
    .unwrap();

    let animation_path = default_assets.join("Animations");
    let target_skeleton_path = default_assets.join("Skeleton").join("skeleton.gltf");
    println!("cargo:rerun-if-changed={}", target_skeleton_path.display());

    if let Ok(entries) = fs::read_dir(&animation_path) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }

    let target_skeleton = skeleton_from_gltf(target_skeleton_path);
    let skeleton_json =
        serde_json::to_string_pretty(&target_skeleton).expect("Failed to serialize skeleton");

    let skeleton_file = out_dir.join("default_skeleton.json");
    fs::write(&skeleton_file, skeleton_json).unwrap();

    println!("Generating {:?}", skeleton_file);

    if !gen_animations {
        return;
    }

    for entry in fs::read_dir(&animation_path).unwrap() {
        let path = entry.unwrap().path();

        let extension = match path.extension().and_then(|e| e.to_str()) {
            Some(ext @ ("gltf" | "glb" | "bvh")) => ext,
            _ => continue,
        };

        let stem = path.file_stem().unwrap().to_string_lossy();

        let animation_name = match DefaultAnimation::from_str(&stem) {
            Ok(animation) => animation,
            Err(_) => continue,
        };

        #[cfg(feature = "quaternius_adjustments")]
        let axis_conversion = Mat4::from_quat(
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
                * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        );

        #[cfg(not(feature = "quaternius_adjustments"))]
        let axis_conversion = Mat4::IDENTITY;

        // Load animation clip.
        let clip = match extension {
            "gltf" | "glb" => load_gltf_animation(&path, axis_conversion),
            "bvh" => load_bvh_animation(&path),
            _ => unreachable!(),
        };

        let out_path = out_dir.join("Animations");
        fs::create_dir_all(&out_path).unwrap();

        let out_path = out_path.join(format!("{:?}.json", animation_name));

        let json = serde_json::to_string_pretty(&clip).expect("Failed to serialize animation");

        fs::write(&out_path, json).unwrap();

        println!("Generating animation {:?}", out_path);
    }
}

fn load_gltf_animation(path: &PathBuf, axis_conversion: Mat4) -> AnimationClip {
    let (document, buffers, _) = gltf::import(path).unwrap();

    let mut animation_data: HashMap<SkinJoint, JointAnimation> = HashMap::new();

    let (mut bind_skeleton, mut _root_transform) =
        bind_skeleton_from_gltf(&document, axis_conversion);

    let inverse_conversion = axis_conversion.inverse();

    for anim in document.animations() {
        for channel in anim.channels() {
            let target = channel.target();

            let node_name = match target.node().name() {
                Some(name) => name,
                None => continue,
            };

            let joint_name = match JointName::resolve_joint(node_name) {
                Some(name) => name,
                None => continue,
            };

            let reader = channel.reader(|buffer| buffers.get(buffer.index()).map(|d| &d.0[..]));

            let times: Vec<f32> = reader.read_inputs().unwrap().collect();

            let entry = animation_data
                .entry(joint_name)
                .or_insert_with(|| JointAnimation {
                    joint: joint_name,
                    translations: Vec::new(),
                    rotations: Vec::new(),
                    scales: Vec::new(),
                });

            match target.property() {
                gltf::animation::Property::Translation => {
                    if let Some(gltf::animation::util::ReadOutputs::Translations(values)) =
                        reader.read_outputs()
                    {
                        for (time, value) in times.iter().zip(values) {
                            let value = Vec3::from_slice(&value);

                            let converted = axis_conversion
                                * Mat4::from_translation(value)
                                * inverse_conversion;

                            let value = converted.to_scale_rotation_translation().2;

                            if entry.joint != SkinJoint::Joint(Pelvis) {
                                continue;
                            };

                            entry.translations.push(Keyframe { time: *time, value });
                        }
                    }
                }

                gltf::animation::Property::Rotation => {
                    if let Some(gltf::animation::util::ReadOutputs::Rotations(values)) =
                        reader.read_outputs()
                    {
                        for (time, value) in times.iter().zip(values.into_f32()) {
                            let value = Quat::from_array(value);

                            let converted =
                                axis_conversion * Mat4::from_quat(value) * inverse_conversion;

                            let rotation = converted.to_scale_rotation_translation().1;

                            entry.rotations.push(Keyframe {
                                time: *time,
                                value: rotation,
                            });
                        }
                    }
                }

                gltf::animation::Property::Scale => {
                    if let Some(gltf::animation::util::ReadOutputs::Scales(values)) =
                        reader.read_outputs()
                    {
                        for (time, value) in times.iter().zip(values) {
                            let value = Vec3::from_slice(&value);

                            let converted =
                                axis_conversion * Mat4::from_scale(value) * inverse_conversion;

                            let scale = converted.to_scale_rotation_translation().0;

                            entry.scales.push(Keyframe {
                                time: *time,
                                value: scale,
                            });
                        }
                    }
                }

                _ => {}
            }
        }
    }
    #[cfg(feature = "quaternius_adjustments")]
    {
        let hip_rotation = Quat::from_rotation_z(std::f32::consts::PI);
        rotate_hip_subtree(
            &mut bind_skeleton,
            &mut animation_data,
            JointName::HipLeft,
            hip_rotation,
        );

        rotate_hip_subtree(
            &mut bind_skeleton,
            &mut animation_data,
            JointName::HipRight,
            hip_rotation,
        );
    }
    AnimationClip {
        bind_skeleton,
        joints: animation_data.into_values().collect(),
    }
}

fn bind_skeleton_from_gltf(document: &gltf::Document, axis_conversion: Mat4) -> (Skeleton, Mat4) {
    let mut skeleton = Skeleton {
        joints: IndexMap::new(),
        root: vec![JointName::Pelvis],
    };

    let mut root_transform = Mat4::IDENTITY;
    let mut has_parent = std::collections::HashSet::new();

    for node in document.nodes() {
        for child in node.children() {
            has_parent.insert(child.index());
        }
    }

    fn recurse(
        node: gltf::Node,
        parent: Option<SkinJoint>,
        skeleton: &mut Skeleton,
        root_transform: &mut Mat4,
        found_root_joint: &mut bool,
        parent_global: Mat4,
    ) {
        let Some(name) = node.name() else {
            return;
        };

        let (translation, rotation, scale) = node.transform().decomposed();

        let node_local = Mat4::from_scale_rotation_translation(
            Vec3::from(scale),
            Quat::from_array(rotation),
            Vec3::from(translation),
        );

        let node_global = parent_global * node_local;

        match JointName::resolve_joint(name) {
            Some(joint_name) => {
                *found_root_joint = true;

                let key = joint_name;

                // Express the transform relative to the actual parent joint.
                // This bakes any skipped helper-node transforms into the local.
                let local_transform = if let Some(parent_key) = parent {
                    let parent_joint_global = skeleton
                        .joints
                        .get(&parent_key)
                        .and_then(|joint| joint.global_transforms.first())
                        .map(|transform| transform.transform)
                        .unwrap_or(parent_global);

                    parent_joint_global.inverse() * node_global
                } else {
                    node_global
                };

                skeleton.joints.insert(
                    key,
                    Joint {
                        name: key,
                        parent,
                        children: Vec::new(),
                        local_transforms: vec![Transform {
                            transform: local_transform,
                            id: Uuid::nil(),
                            rank: 0,
                            name: String::new(),
                        }],
                        global_transforms: vec![Transform {
                            transform: node_global,
                            id: Uuid::nil(),
                            rank: 0,
                            name: String::new(),
                        }],
                    },
                );

                if let Some(parent_key) = parent {
                    if let Some(parent_joint) = skeleton.joints.get_mut(&parent_key) {
                        parent_joint.children.push(key);
                    }
                }

                for child in node.children() {
                    recurse(
                        child,
                        Some(key),
                        skeleton,
                        root_transform,
                        found_root_joint,
                        node_global,
                    );
                }
            }

            None => {
                if !*found_root_joint {
                    *root_transform = node_local * *root_transform;
                }

                for child in node.children() {
                    recurse(
                        child,
                        parent,
                        skeleton,
                        root_transform,
                        found_root_joint,
                        node_global,
                    );
                }
            }
        }
    }

    for node in document.nodes() {
        if has_parent.contains(&node.index()) {
            continue;
        }

        let mut found_root_joint = false;

        recurse(
            node,
            None,
            &mut skeleton,
            &mut root_transform,
            &mut found_root_joint,
            Mat4::IDENTITY,
        );

        if found_root_joint {
            break;
        }
    }

    let inverse_conversion = axis_conversion.inverse();
    let converted_root = axis_conversion * root_transform * inverse_conversion;

    for joint in skeleton.joints.values_mut() {
        let local = joint.local_transforms[0].transform;

        joint.local_transforms[0].transform = axis_conversion * local * inverse_conversion;
    }

    if let Some(pelvis) = skeleton.joints.get_mut(&SkinJoint::Joint(Pelvis)) {
        pelvis.local_transforms[0].transform =
            converted_root * pelvis.local_transforms[0].transform;
    }

    calculate_bind_globals(&mut skeleton);

    (skeleton, converted_root)
}

fn calculate_bind_globals(skeleton: &mut Skeleton) {
    fn recurse(key: SkinJoint, skeleton: &mut Skeleton, parent_global: Mat4) {
        let local = skeleton.joints[&key].local_transforms[0].transform;
        let global = parent_global * local;

        let joint = skeleton.joints.get_mut(&key).unwrap();

        joint.global_transforms = vec![Transform {
            transform: global,
            id: Uuid::nil(),
            rank: 0,
            name: String::new(),
        }];

        let children = joint.children.clone();

        for child in children {
            recurse(child, skeleton, global);
        }
    }

    let roots: Vec<SkinJoint> = skeleton
        .joints
        .values()
        .filter(|joint| joint.parent.is_none())
        .map(|joint| joint.name)
        .collect();

    for root in roots {
        recurse(root, skeleton, Mat4::IDENTITY);
    }
}

fn load_bvh_animation(path: &PathBuf) -> AnimationClip {
    let bvh_file = File::open(path).unwrap();

    let bvh = bvh_anim::from_reader(BufReader::new(bvh_file)).unwrap();

    let mut animation_data: HashMap<SkinJoint, JointAnimation> = HashMap::new();

    let axis_correction = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    for (frame_idx, frame) in bvh.frames().enumerate() {
        let frame_time = bvh.frame_time().as_secs_f32();

        let time = frame_idx as f32 * frame_time;

        for joint in bvh.joints() {
            let joint_name =
                match JointName::resolve_joint_and_collision(joint.data().name().to_str().unwrap())
                {
                    Some(j) => j,
                    None => continue,
                };

            let mut translation = Vec3::ZERO;
            let mut rotation = Quat::IDENTITY;

            for channel in joint.data().channels() {
                let value = frame.get(channel).unwrap();

                let radians = value.to_radians();

                match channel.channel_type() {
                    ChannelType::PositionX => translation.x = *value,
                    ChannelType::PositionY => translation.y = *value,
                    ChannelType::PositionZ => translation.z = *value,

                    ChannelType::RotationX => rotation *= Quat::from_rotation_x(radians),

                    ChannelType::RotationY => rotation *= Quat::from_rotation_y(radians),

                    ChannelType::RotationZ => rotation *= Quat::from_rotation_z(radians),
                }
            }

            let corrected_translation = (axis_correction * translation) * 0.01;

            let corrected_rotation = match joint_name {
                SkinJoint::Joint(Pelvis) => rotation,
                _ => axis_correction * rotation * axis_correction.inverse(),
            };

            let entry = animation_data
                .entry(joint_name)
                .or_insert_with(|| JointAnimation {
                    joint: joint_name,
                    translations: Vec::new(),
                    rotations: Vec::new(),
                    scales: Vec::new(),
                });

            if joint_name == SkinJoint::Joint(Pelvis) {
                entry.translations.push(Keyframe {
                    time,
                    value: corrected_translation,
                });
            }

            entry.rotations.push(Keyframe {
                time,
                value: corrected_rotation,
            });

            entry.scales.push(Keyframe {
                time,
                value: Vec3::ONE,
            });
        }
    }

    AnimationClip {
        bind_skeleton: Skeleton::default(),
        joints: animation_data.into_values().collect(),
    }
}

/// Generate the default skeleton from the GLTF file.
///
/// The GLTF has an incorrect axis basis. It remains in GLTF format
/// for viewability and editability, but the generated JSON is
/// converted to the OpenSim orientation.
fn skeleton_from_gltf(skeleton_path: PathBuf) -> Skeleton {
    let (document, _, _) = gltf::import(&skeleton_path)
        .unwrap_or_else(|_| panic!("Failed to load skeleton {:?}", skeleton_path));

    let nodes: Vec<Node> = document.nodes().collect();
    let mut joints = IndexMap::new();

    let pelvis = nodes
        .iter()
        .find(|node| node.name() == Some("mPelvis"))
        .unwrap_or_else(|| panic!("Failed to find mPelvis in skeleton {:?}", skeleton_path));

    build_joint_recursive(pelvis.index(), None, &nodes, Mat4::IDENTITY, &mut joints);

    let axis_conversion = Mat4::from_quat(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));

    let inverse_conversion = axis_conversion.inverse();

    for joint in joints.values_mut() {
        for transform in &mut joint.local_transforms {
            transform.transform = axis_conversion * transform.transform * inverse_conversion;
        }
    }

    let mut skeleton = Skeleton {
        joints,
        root: vec![JointName::Pelvis],
    };

    calculate_bind_globals(&mut skeleton);

    skeleton
}

fn build_joint_recursive(
    index: usize,
    parent: Option<SkinJoint>,
    nodes: &[Node],
    parent_global: Mat4,
    joints: &mut IndexMap<SkinJoint, Joint>,
) {
    let node = &nodes[index];

    let node_name = node
        .name()
        .unwrap_or_else(|| panic!("Skeleton node {} has no name", index));

    let (translation, rotation, scale) = node.transform().decomposed();

    let node_local = Mat4::from_scale_rotation_translation(
        Vec3::from(scale),
        Quat::from_array(rotation),
        Vec3::from(translation),
    );

    let node_global = parent_global * node_local;

    let Some(key) = JointName::resolve_joint_and_collision(node_name) else {
        // Skip helper nodes, but retain their transforms in descendants.
        for child in node.children() {
            build_joint_recursive(child.index(), parent, nodes, node_global, joints);
        }

        return;
    };

    if joints.contains_key(&key) {
        return;
    }

    let local = if let Some(parent_key) = parent {
        let parent_global = joints
            .get(&parent_key)
            .and_then(|joint| joint.global_transforms.first())
            .map(|transform| transform.transform)
            .unwrap_or(parent_global);

        parent_global.inverse() * node_global
    } else {
        node_global
    };

    let mut children = Vec::new();

    // Insert before traversing children so they can resolve this parent.
    joints.insert(
        key,
        Joint {
            name: key,
            parent,
            children: Vec::new(),
            global_transforms: vec![Transform {
                name: "Default".to_string(),
                id: Uuid::nil(),
                transform: node_global,
                rank: 0,
            }],
            local_transforms: vec![Transform {
                name: "Default".to_string(),
                id: Uuid::nil(),
                transform: local,
                rank: 0,
            }],
        },
    );

    for child in node.children() {
        let child_index = child.index();

        build_joint_recursive(child_index, Some(key), nodes, node_global, joints);

        if let Some(child_name) = child.name() {
            if let Some(child_key) = JointName::resolve_joint_and_collision(child_name) {
                if joints
                    .get(&child_key)
                    .is_some_and(|joint| joint.parent == Some(key))
                {
                    children.push(child_key);
                }
            }
        }
    }

    if let Some(joint) = joints.get_mut(&key) {
        joint.children = children;
    }
}

#[cfg(feature = "quaternius_adjustments")]
fn rotate_hip_subtree(
    skeleton: &mut Skeleton,
    animation_data: &mut HashMap<SkinJoint, JointAnimation>,
    hip: JointName,
    rotation: Quat,
) {
    let correction = Mat4::from_quat(rotation);

    let joints = match hip {
        JointName::HipLeft => [JointName::HipLeft, JointName::KneeLeft, JointName::FootLeft],
        JointName::HipRight => [
            JointName::HipRight,
            JointName::KneeRight,
            JointName::FootRight,
        ],
        _ => return,
    };

    let hip_key = SkinJoint::Joint(hip);
    let hip_joint_key = SkinJoint::Joint(joints[0]);
    let knee_key = SkinJoint::Joint(joints[1]);
    let foot_key = SkinJoint::Joint(joints[2]);

    // Rotate the hip's local transform.
    {
        let local = skeleton.joints[&hip_joint_key].local_transforms[0].transform;

        skeleton
            .joints
            .get_mut(&hip_joint_key)
            .unwrap()
            .local_transforms[0]
            .transform = local * correction;

        if let Some(animation) = animation_data.get_mut(&SkinJoint::Joint(joints[0])) {
            for keyframe in &mut animation.rotations {
                let corrected = Mat4::from_quat(keyframe.value) * correction;
                keyframe.value = corrected.to_scale_rotation_translation().1;
            }
        }
    }

    // Recalculate globals for hip -> knee -> foot.
    let parent_global = skeleton.joints[&hip_key]
        .parent
        .map(|parent| {
            skeleton.joints[&parent]
                .global_transforms
                .last()
                .unwrap()
                .transform
        })
        .unwrap_or(Mat4::IDENTITY);

    let hip_local = skeleton.joints[&hip_joint_key]
        .local_transforms
        .last()
        .unwrap()
        .transform;
    let hip_global = parent_global * hip_local;

    skeleton
        .joints
        .get_mut(&hip_joint_key)
        .unwrap()
        .global_transforms
        .last_mut()
        .unwrap()
        .transform = hip_global;

    let knee_local = skeleton.joints[&knee_key]
        .local_transforms
        .last()
        .unwrap()
        .transform;
    let knee_global = hip_global * knee_local;

    skeleton
        .joints
        .get_mut(&knee_key)
        .unwrap()
        .global_transforms
        .last_mut()
        .unwrap()
        .transform = knee_global;

    let foot_local = skeleton.joints[&foot_key]
        .local_transforms
        .last()
        .unwrap()
        .transform;
    let foot_global = knee_global * foot_local;

    skeleton
        .joints
        .get_mut(&foot_key)
        .unwrap()
        .global_transforms
        .last_mut()
        .unwrap()
        .transform = foot_global;
    calculate_bind_globals(skeleton);
}
