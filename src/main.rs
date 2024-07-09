use std::f32::consts::PI;

use bevy::core::FrameCount;
use bevy::core_pipeline::bloom::BloomSettings;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::system::EntityCommand;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::render::camera::Exposure;
use bevy::window::*;
use bevy_asset_loader::asset_collection::AssetCollection;
use bevy_asset_loader::loading_state::config::ConfigureLoadingState;
use bevy_asset_loader::loading_state::LoadingState;
use bevy_asset_loader::loading_state::LoadingStateAppExt;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_scene_hook::HookPlugin;
use bevy_scene_hook::HookedSceneBundle;
use bevy_scene_hook::SceneHook;
use bevy_xpbd_3d::components::CollisionLayers;
use bevy_xpbd_3d::components::LinearVelocity;
use bevy_xpbd_3d::components::RigidBody;
use bevy_xpbd_3d::plugins::PhysicsDebugPlugin;
use bevy_xpbd_3d::plugins::PhysicsPlugins;
use bevy_xpbd_3d::prelude::Collider;
use bevy_xpbd_3d::prelude::ColliderParent;
use bevy_xpbd_3d::prelude::Collision;
use bevy_xpbd_3d::prelude::PhysicsGizmos;
use bevy_xpbd_3d::prelude::PhysicsLayer;
use bevy_xpbd_3d::resources::Gravity;

#[derive(Clone, Eq, PartialEq, Debug, Hash, Default, States)]
pub enum State {
    #[default]
    Load,
    Play,
}

#[derive(AssetCollection, Resource)]
pub struct BlenderAssets {
    #[asset(path = "rock.glb#Mesh0/Primitive0")]
    pub ball: Handle<Mesh>,

    #[asset(path = "rock.glb#Mesh1/Primitive0")]
    pub spike: Handle<Mesh>,

    #[asset(path = "rock.glb#Scene0")]
    pub rock: Handle<Scene>,

    #[asset(path = "ammo.glb#Scene0")]
    pub ammo: Handle<Scene>,
}

fn main() {
    App::new()
        .init_state::<State>()
        .insert_resource(Gravity::ZERO)
        .add_loading_state(
            LoadingState::new(State::Load)
                .continue_to_state(State::Play)
                .load_collection::<BlenderAssets>(),
        )
        .add_plugins((
            PhysicsPlugins::default(),
            PhysicsDebugPlugin::default(),
            HookPlugin,
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Scene Viewer".into(),
                        resolution: (450., 800.).into(),
                        resizable: false,
                        window_theme: Some(WindowTheme::Dark),
                        present_mode: PresentMode::AutoVsync,
                        enabled_buttons: EnabledButtons {
                            minimize: false,
                            maximize: false,
                            ..Default::default()
                        },
                        // This will spawn an invisible window
                        // The window will be made visible in the make_visible() system after 3 frames.
                        // This is useful when you want to avoid the white window that shows up before the GPU is ready to render the app.
                        visible: false,
                        ..default()
                    }),
                    ..default()
                })
                .set(LogPlugin {
                    update_subscriber: None,
                    filter: "info,wgpu=error,winit=error".into(),
                    level: bevy::log::Level::INFO,
                }),
            WorldInspectorPlugin::new(),
        ))
        .insert_gizmo_group(
            PhysicsGizmos::default(),
            GizmoConfig {
                enabled: true,
                ..default()
            },
        )
        .add_systems(Startup, setup_camera)
        .add_systems(Update, (close_on_esc, despawn_delayed, make_visible))
        .add_systems(OnEnter(State::Play), setup_scene)
        .add_systems(Update, (fire, collide).run_if(in_state(State::Play)))
        .run();
}

fn make_visible(mut window: Query<&mut Window>, frames: Res<FrameCount>) {
    // The delay may be different for your app or system.
    if frames.0 == 3 {
        // At this point the gpu is ready to show the app so we can make the window visible.
        // Alternatively, you could toggle the visibility in Startup.
        // It will work, but it will have one white frame before it starts rendering
        window.single_mut().visible = true;
    }
}

#[derive(PhysicsLayer)]
enum Layer {
    Ammo,
    Object,
}

impl Layer {
    pub fn config(&self) -> CollisionLayers {
        match self {
            Layer::Ammo => CollisionLayers::new([Layer::Ammo], [Layer::Object]),
            Layer::Object => CollisionLayers::new([Layer::Object], [Layer::Ammo]),
        }
    }
}
pub struct Collidable {
    layer: Layer,
}

impl EntityCommand for Collidable {
    fn apply(self, entity: Entity, world: &mut World) {
        let first_child = world.query::<&Children>().get(world, entity).unwrap()[0];
        let handle = world.entity(first_child).get::<Handle<Mesh>>().unwrap();
        let meshes = world.get_resource::<Assets<Mesh>>().unwrap();
        let collider = Collider::trimesh_from_mesh(meshes.get(handle).unwrap()).unwrap();
        world
            .entity_mut(entity)
            .insert((collider, self.layer.config()));
    }
}

fn setup_scene(mut commands: Commands, assets: Res<BlenderAssets>) {
    commands.insert_resource(Trigger(Timer::from_seconds(4.0, TimerMode::Repeating)));

    info!("setup scene");
    commands.spawn((
        HookedSceneBundle {
            scene: SceneBundle {
                scene: assets.rock.clone_weak(),
                transform: Transform::from_scale(Vec3::splat(4.0))
                    .with_translation(Vec3::new(15.0, 15.0, 0.0)),
                ..default()
            },
            hook: SceneHook::new(move |entity, cmds| {
                let name = entity.get::<Name>().map(|t| t.as_str());
                match name {
                    Some("ball") => cmds.add(Collidable {
                        layer: Layer::Object,
                    }),
                    _ => cmds,
                };
            }),
        },
        RigidBody::Kinematic,
    ));
}

#[derive(Resource, Deref, DerefMut)]
struct Trigger(pub Timer);

fn fire(
    time: Res<Time>,
    mut trigger: ResMut<Trigger>,
    mut commands: Commands,
    assets: Res<BlenderAssets>,
) {
    if !trigger.tick(time.delta()).finished() {
        return;
    }

    let velocity = Vec3::new(10.0, 10.0, 0.0);
    let transform =
        Transform::from_scale(Vec3::splat(2.0)).with_rotation(rotation_between(Vec3::Y, velocity));

    info!("fire ammo");
    commands.spawn((
        HookedSceneBundle {
            scene: SceneBundle {
                scene: assets.ammo.clone_weak(),
                transform,
                ..default()
            },
            hook: SceneHook::new(move |entity, cmds| {
                let name = entity.get::<Name>().map(|name| name.as_str());
                match name {
                    Some("collider") => cmds
                        .insert(Visibility::Hidden)
                        .add(Collidable { layer: Layer::Ammo }),
                    _ => cmds,
                };
            }),
        },
        LinearVelocity(velocity),
        RigidBody::Kinematic,
        DelayedDespawn::after(8.0),
    ));
}

pub fn collide(
    entities: Query<(&ColliderParent, &GlobalTransform, &Collider)>,
    mut collision_event_reader: EventReader<Collision>,
    assets: Res<BlenderAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for Collision(contacts) in collision_event_reader.read() {
        if contacts.during_previous_frame {
            continue;
        }
        let Ok([(_, transform1, _), (_, transform2, _)]) =
            entities.get_many([contacts.entity1, contacts.entity2])
        else {
            continue;
        };
        let sum = contacts
            .manifolds
            .iter()
            .fold((Vec3::ZERO, Vec3::ZERO), |acc, manifold| {
                let sum = manifold
                    .contacts
                    .iter()
                    .fold((Vec3::ZERO, Vec3::ZERO), |a, v| {
                        (a.0 + v.point1, a.1 + v.point2)
                    });
                let count = manifold.contacts.len() as f32;
                let (point1, point2) = (sum.0 / count, sum.1 / count);
                (acc.0 + point1, acc.1 + point2)
            });
        let count = contacts.manifolds.len() as f32;
        let (t1, t2) = (
            transform1.compute_transform(),
            transform2.compute_transform(),
        );
        let (point1, point2) = (
            t1.translation + t1.rotation * (sum.0 / count),
            t2.translation + t2.rotation * (sum.1 / count),
        );

        // BUG: why point1 and point2 are far from each other?
        info!("collide at {:?} {:?}", point1, point2);

        for p in [point1, point2] {
            commands.spawn((
                MaterialMeshBundle {
                    mesh: assets.ball.clone_weak(),
                    material: materials.add(StandardMaterial {
                        base_color: Color::BLUE,
                        emissive: Color::BLUE * 500.0,
                        ..default()
                    }),
                    transform: Transform::from_translation(p).with_scale(Vec3::splat(3.0)),
                    ..default()
                },
                DelayedDespawn::after(1.0),
            ));
        }
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3dBundle {
            camera: Camera {
                hdr: true,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            exposure: Exposure::BLENDER,
            camera_3d: Camera3d::default(),
            tonemapping: Tonemapping::TonyMcMapface,
            transform: Transform::from_translation(Vec3::new(0., 0., 100.))
                .looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        BloomSettings::default(),
    ));

    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: light_consts::lux::OVERCAST_DAY,
            shadows_enabled: false,
            ..default()
        },
        transform: Transform {
            translation: Vec3::new(4., 4., 10.),
            rotation: Quat::from_rotation_y(PI / 4.),
            ..default()
        },
        ..default()
    });
}

const EPSILON: f32 = 0.001;

trait ZeroCheck {
    fn almost_zero(&self) -> bool;
}

impl ZeroCheck for Vec3 {
    fn almost_zero(&self) -> bool {
        self.x.abs() < EPSILON && self.y.abs() < EPSILON && self.z.abs() < EPSILON
    }
}

pub fn rotation_between(from: Vec3, to: Vec3) -> Quat {
    if from.almost_zero() || to.almost_zero() {
        return Quat::IDENTITY;
    }
    let normalized_a = from.normalize();
    let normalized_b = to.normalize();
    let dot_product = normalized_a.dot(normalized_b);

    if dot_product >= 1.0 {
        // Vectors are already aligned, no rotation needed.
        Quat::IDENTITY
    } else if dot_product <= -1.0 {
        // Vectors are opposite, a 180-degree rotation is needed.
        // Choose any axis perpendicular to `a` for the rotation.
        let axis = Vec3::X.cross(normalized_a).normalize();
        Quat::from_axis_angle(axis, std::f32::consts::PI)
    } else {
        // General case: Calculate the rotation axis and angle.
        let axis = normalized_a.cross(normalized_b).normalize();
        let angle = dot_product.acos();
        Quat::from_axis_angle(axis, angle)
    }
}

#[derive(Component)]
pub struct DelayedDespawn {
    timer: Timer,
}

impl DelayedDespawn {
    pub fn after(seconds: f32) -> DelayedDespawn {
        DelayedDespawn {
            timer: Timer::from_seconds(seconds, TimerMode::Once),
        }
    }
}

fn despawn_delayed(
    time: Res<Time>,
    mut query: Query<(Entity, &mut DelayedDespawn)>,
    mut commands: Commands,
) {
    for (entity, mut dd) in query.iter_mut() {
        if dd.timer.tick(time.delta()).finished() {
            commands.entity(entity).despawn_recursive();
        }
    }
}
