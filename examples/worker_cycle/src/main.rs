use bevy::prelude::*;
use saddle_ai_saddle_ai_goap::{
    ActionDefinition, ActionDispatched, ActionExecutionReport, ActionExecutionStatus, ActionId,
    GoalDefinition, GoalId, GoapAgent, GoapHooks, GoapLibrary, GoapPlugin, GoapSystems,
    SensorDefinition, SensorId, SensorInterval, SensorOutput,
};

#[derive(Component, Clone, Copy, Default)]
struct WorkerInventory {
    has_ore: bool,
    has_ingot: bool,
    deposited: bool,
}

#[derive(Component)]
struct WorkerAgent;

#[derive(Resource)]
struct WorkbenchAvailability(bool);

fn main() {
    App::new()
        .insert_resource(WorkbenchAvailability(true))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "goap worker_cycle".into(),
                resolution: (1100, 640).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(GoapPlugin::always_on(Update))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            execute_worker_actions
                .after(GoapSystems::Dispatch)
                .before(GoapSystems::Monitor),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut library: ResMut<GoapLibrary>,
    mut hooks: ResMut<GoapHooks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((Name::new("Camera"), Camera2d));

    let mut domain = saddle_ai_goap::GoapDomainDefinition::new("worker_cycle");
    let has_ore = domain.add_bool_key("has_ore", Some("worker carries ore".into()), Some(false));
    let has_ingot = domain.add_bool_key(
        "has_ingot",
        Some("worker carries a finished ingot".into()),
        Some(false),
    );
    let deposited = domain.add_bool_key(
        "deposited",
        Some("worker deposited the crafted ingot".into()),
        Some(false),
    );
    let workbench_available = domain.add_bool_key(
        "workbench_available",
        Some("a workbench can process ore right now".into()),
        Some(true),
    );
    domain.add_local_sensor(
        SensorDefinition::new(
            SensorId(0),
            "worker_inventory",
            saddle_ai_goap::SensorScope::Local,
            "worker_inventory",
            [has_ore, has_ingot, deposited],
        )
        .with_interval(SensorInterval::every(0.0)),
    );
    domain.add_global_sensor(
        SensorDefinition::new(
            SensorId(0),
            "workbench_sensor",
            saddle_ai_goap::SensorScope::Global,
            "workbench_sensor",
            [workbench_available],
        )
        .with_interval(SensorInterval::every(0.0)),
    );
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "deliver crafted ingot")
            .with_priority(15)
            .with_desired_state([saddle_ai_goap::FactCondition::equals_bool(deposited, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "gather ore", "gather_ore")
            .with_effects([saddle_ai_goap::FactEffect::set_bool(has_ore, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(1), "smelt ore", "smelt_ore")
            .with_preconditions([
                saddle_ai_goap::FactCondition::equals_bool(has_ore, true),
                saddle_ai_goap::FactCondition::equals_bool(workbench_available, true),
            ])
            .with_effects([
                saddle_ai_goap::FactEffect::set_bool(has_ore, false),
                saddle_ai_goap::FactEffect::set_bool(has_ingot, true),
            ]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(2), "deposit ingot", "deposit_ingot")
            .with_preconditions([saddle_ai_goap::FactCondition::equals_bool(has_ingot, true)])
            .with_effects([
                saddle_ai_goap::FactEffect::set_bool(has_ingot, false),
                saddle_ai_goap::FactEffect::set_bool(deposited, true),
            ]),
    );
    let domain_id = library.register(domain);

    hooks.register_local_sensor("worker_inventory", move |world, ctx| {
        let inventory = world
            .get::<WorkerInventory>(ctx.entity)
            .cloned()
            .unwrap_or_default();
        SensorOutput::new([
            saddle_ai_goap::FactPatch::set_bool(has_ore, inventory.has_ore),
            saddle_ai_goap::FactPatch::set_bool(has_ingot, inventory.has_ingot),
            saddle_ai_goap::FactPatch::set_bool(deposited, inventory.deposited),
        ])
    });
    hooks.register_global_sensor("workbench_sensor", move |world, _ctx| {
        SensorOutput::new([saddle_ai_goap::FactPatch::set_bool(
            workbench_available,
            world.resource::<WorkbenchAvailability>().0,
        )])
    });

    commands.spawn((
        Name::new("Worker"),
        WorkerAgent,
        WorkerInventory::default(),
        GoapAgent::new(domain_id),
        Mesh2d(meshes.add(Rectangle::new(72.0, 72.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.22, 0.58, 0.86))),
        Transform::from_xyz(-260.0, 0.0, 0.0),
    ));

    for (name, x, color) in [
        ("Ore Node", -40.0, Color::srgb(0.48, 0.35, 0.27)),
        ("Workbench", 180.0, Color::srgb(0.62, 0.58, 0.28)),
        ("Depot", 360.0, Color::srgb(0.30, 0.74, 0.44)),
    ] {
        commands.spawn((
            Name::new(name),
            Mesh2d(meshes.add(Rectangle::new(110.0, 90.0))),
            MeshMaterial2d(materials.add(color)),
            Transform::from_xyz(x, -120.0, 0.0),
        ));
    }
}

fn execute_worker_actions(
    mut dispatched: MessageReader<ActionDispatched>,
    mut reports: MessageWriter<ActionExecutionReport>,
    mut workers: Query<&mut WorkerInventory, With<WorkerAgent>>,
) {
    for message in dispatched.read() {
        let Ok(mut inventory) = workers.get_mut(message.entity) else {
            continue;
        };

        match message.executor.as_str() {
            "gather_ore" => {
                inventory.has_ore = true;
                inventory.deposited = false;
            }
            "smelt_ore" => {
                inventory.has_ore = false;
                inventory.has_ingot = true;
            }
            "deposit_ingot" => {
                inventory.has_ingot = false;
                inventory.deposited = true;
            }
            _ => continue,
        }

        reports.write(ActionExecutionReport::new(
            message.entity,
            message.ticket,
            ActionExecutionStatus::Success,
        ));
    }
}
