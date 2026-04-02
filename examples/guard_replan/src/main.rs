use bevy::prelude::*;
use saddle_ai_saddle_ai_goap::{
    ActionDefinition, ActionDispatched, ActionExecutionReport, ActionExecutionStatus, ActionId,
    GoalDefinition, GoalId, GoapAgent, GoapHooks, GoapLibrary, GoapPlugin, GoapSystems, HookKey,
    SensorDefinition, SensorId, SensorInterval, SensorOutput, TargetCandidate, TargetToken,
};

#[derive(Component)]
struct GuardAgent;

#[derive(Component)]
struct GuardTarget;

#[derive(Resource, Default)]
struct GuardActionState {
    elapsed: f32,
    active_entity: Option<Entity>,
    active_ticket: Option<u64>,
    active_target: Option<Entity>,
    action_started_at: f32,
    removed_first_target: bool,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "goap guard_replan".into(),
                resolution: (1100, 640).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(GuardActionState::default())
        .add_plugins(GoapPlugin::always_on(Update))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                remember_dispatch
                    .after(GoapSystems::Dispatch)
                    .before(GoapSystems::Monitor),
                tick_guard_action.after(remember_dispatch),
            ),
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

    let mut domain = saddle_ai_goap::GoapDomainDefinition::new("guard_replan");
    let has_target =
        domain.add_bool_key("has_target", Some("live targets exist".into()), Some(false));
    let neutralized = domain.add_bool_key(
        "neutralized",
        Some("selected target neutralized".into()),
        Some(false),
    );
    domain.add_local_sensor(
        SensorDefinition::new(
            SensorId(0),
            "guard_sensor",
            saddle_ai_goap::SensorScope::Local,
            "guard_sensor",
            [has_target],
        )
        .with_interval(SensorInterval::every(0.0)),
    );
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "neutralize target")
            .with_priority(20)
            .with_desired_state([saddle_ai_goap::FactCondition::equals_bool(
                neutralized,
                true,
            )]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "use target", HookKey::new("use_target"))
            .with_preconditions([saddle_ai_goap::FactCondition::equals_bool(has_target, true)])
            .with_target("enemy", "guard_targets")
            .with_effects([saddle_ai_goap::FactEffect::set_bool(neutralized, true)]),
    );
    let domain_id = library.register(domain);

    hooks.register_local_sensor("guard_sensor", move |world, _ctx| {
        let has_any_target = world
            .query_filtered::<Entity, With<GuardTarget>>()
            .iter(world)
            .next()
            .is_some();
        SensorOutput::new([saddle_ai_goap::FactPatch::set_bool(
            has_target,
            has_any_target,
        )])
    });
    hooks.register_target_provider("guard_targets", |world, _ctx| {
        let mut query = world.query_filtered::<(Entity, &Transform), With<GuardTarget>>();
        query
            .iter(world)
            .enumerate()
            .map(|(index, (entity, transform))| {
                TargetCandidate::new(
                    TargetToken(entity.to_bits()),
                    format!("Target {}", index + 1),
                )
                .with_cost_bias(index as u32)
                .with_debug_position(transform.translation)
            })
            .collect::<Vec<_>>()
    });

    commands.spawn((
        Name::new("Guard"),
        GuardAgent,
        GoapAgent::new(domain_id),
        Mesh2d(meshes.add(Rectangle::new(70.0, 70.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.95, 0.62, 0.18))),
        Transform::from_xyz(-340.0, 0.0, 0.0),
    ));

    for (index, x) in [140.0_f32, 320.0].into_iter().enumerate() {
        commands.spawn((
            Name::new(format!("Target {}", index + 1)),
            GuardTarget,
            Mesh2d(meshes.add(Rectangle::new(60.0, 60.0))),
            MeshMaterial2d(materials.add(Color::srgb(0.86, 0.18, 0.24))),
            Transform::from_xyz(x, 0.0, 0.0),
        ));
    }
}

fn remember_dispatch(
    mut dispatched: MessageReader<ActionDispatched>,
    mut state: ResMut<GuardActionState>,
) {
    for message in dispatched.read() {
        if message.executor.as_str() != "use_target" {
            continue;
        }
        state.active_entity = Some(message.entity);
        state.active_ticket = Some(message.ticket);
        state.active_target = message
            .target
            .as_ref()
            .and_then(|target| Entity::try_from_bits(target.token.0));
        state.action_started_at = state.elapsed;
    }
}

fn tick_guard_action(
    time: Res<Time>,
    mut state: ResMut<GuardActionState>,
    mut reports: MessageWriter<ActionExecutionReport>,
    mut commands: Commands,
) {
    state.elapsed += time.delta_secs();
    let (Some(entity), Some(ticket)) = (state.active_entity, state.active_ticket) else {
        return;
    };
    if state.elapsed - state.action_started_at < 0.8 {
        return;
    }

    if !state.removed_first_target {
        if let Some(target) = state.active_target.take() {
            commands.entity(target).despawn();
        }
        state.active_ticket = None;
        state.active_entity = None;
        state.removed_first_target = true;
        reports.write(ActionExecutionReport::new(
            entity,
            ticket,
            ActionExecutionStatus::Failure {
                reason: "target disappeared before use".into(),
            },
        ));
        return;
    }

    reports.write(ActionExecutionReport::new(
        entity,
        ticket,
        ActionExecutionStatus::Success,
    ));
    if let Some(target) = state.active_target.take() {
        commands.entity(target).despawn();
    }
    state.active_ticket = None;
    state.active_entity = None;
}
