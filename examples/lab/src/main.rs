#[cfg(feature = "e2e")]
mod e2e;
#[cfg(feature = "e2e")]
mod scenarios;

use bevy::prelude::*;
#[cfg(feature = "dev")]
use bevy::remote::{RemotePlugin, http::RemoteHttpPlugin};
#[cfg(feature = "dev")]
use bevy_brp_extras::BrpExtrasPlugin;
use saddle_ai_saddle_ai_goap::{
    ActionDefinition, ActionDispatched, ActionExecutionReport, ActionExecutionStatus, ActionId,
    GoalDefinition, GoalId, GoapAgent, GoapDebugSnapshot, GoapHooks, GoapLibrary, GoapPlan,
    GoapPlannerScheduler, GoapPlugin, GoapRuntime, GoapSystems, PlanCompleted, PlanInvalidated,
    SelectedGoal, SensorDefinition, SensorId, SensorInterval, SensorOutput, TargetCandidate,
    TargetToken,
};

const GUARD_ACTION_DURATION: f32 = 0.45;
const WORKER_ACTION_DURATION: f32 = 0.35;
const WORKBENCH_RESTORE_DELAY: f32 = 1.0;

const GUARD_NAME: &str = "Guard Agent";
const WORKER_NAME: &str = "Worker Agent";
const GUARD_TARGET_A_NAME: &str = "Guard Target A";
const GUARD_TARGET_B_NAME: &str = "Guard Target B";

#[derive(Component)]
struct GuardAgent;

#[derive(Component)]
struct GuardTarget;

#[derive(Component)]
struct WorkerAgent;

#[derive(Component, Clone, Copy, Default)]
pub struct WorkerInventory {
    pub has_ore: bool,
    pub has_ingot: bool,
    pub deposited: bool,
}

#[derive(Component)]
struct LabOverlay;

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct WorkbenchAvailability(pub bool);

#[derive(Resource, Debug, Clone, Default)]
pub struct GoapLabDiagnostics {
    pub guard_plan_starts: u32,
    pub guard_plan_invalidations: u32,
    pub guard_plan_completions: u32,
    pub guard_last_invalidation: Option<String>,
    pub guard_targets_remaining: usize,
    pub guard_status: String,
    pub worker_plan_starts: u32,
    pub worker_plan_invalidations: u32,
    pub worker_plan_completions: u32,
    pub worker_last_invalidation: Option<String>,
    pub worker_status: String,
    pub worker_has_ore: bool,
    pub worker_has_ingot: bool,
    pub worker_deposited: bool,
    pub workbench_available: bool,
    pub planner_queue_depth: usize,
}

#[derive(Resource, Debug, Clone, Default)]
struct GuardActionState {
    active_entity: Option<Entity>,
    active_ticket: Option<u64>,
    active_target: Option<Entity>,
    started_at: f32,
    removed_first_target: bool,
}

#[derive(Resource, Debug, Clone, Default)]
struct WorkerActionState {
    active_entity: Option<Entity>,
    active_ticket: Option<u64>,
    active_executor: Option<String>,
    started_at: f32,
    blocked_workbench_once: bool,
    restore_workbench_at: Option<f32>,
}

fn main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.045, 0.055, 0.07)));
    app.insert_resource(WorkbenchAvailability(true));
    app.insert_resource(GoapLabDiagnostics::default());
    app.insert_resource(GuardActionState::default());
    app.insert_resource(WorkerActionState::default());
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "goap crate-local lab".into(),
            resolution: (1520, 900).into(),
            ..default()
        }),
        ..default()
    }));
    #[cfg(feature = "dev")]
    app.add_plugins(RemotePlugin::default());
    #[cfg(feature = "dev")]
    app.add_plugins(BrpExtrasPlugin::with_http_plugin(
        RemoteHttpPlugin::default(),
    ));
    #[cfg(feature = "e2e")]
    app.add_plugins(e2e::GoapLabE2EPlugin);
    app.add_plugins(GoapPlugin::always_on(Update));
    app.add_systems(Startup, setup);
    app.add_systems(
        Update,
        (
            remember_guard_dispatch
                .after(GoapSystems::Dispatch)
                .before(GoapSystems::Monitor),
            remember_worker_dispatch
                .after(GoapSystems::Dispatch)
                .before(GoapSystems::Monitor),
            advance_guard_actions
                .after(remember_guard_dispatch)
                .before(GoapSystems::Monitor),
            advance_worker_actions
                .after(remember_worker_dispatch)
                .before(GoapSystems::Monitor),
            record_goap_messages.after(GoapSystems::Debug),
            update_diagnostics.after(record_goap_messages),
            update_overlay.after(update_diagnostics),
        ),
    );
    app.run();
}

fn setup(
    mut commands: Commands,
    mut library: ResMut<GoapLibrary>,
    mut hooks: ResMut<GoapHooks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((Name::new("Lab Camera"), Camera2d));

    let guard_domain = configure_guard_domain(&mut hooks, &mut library);
    let worker_domain = configure_worker_domain(&mut hooks, &mut library);

    commands.spawn((
        Name::new("Guard Lane"),
        Mesh2d(meshes.add(Rectangle::new(620.0, 300.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.11, 0.12, 0.18))),
        Transform::from_xyz(-300.0, 150.0, -5.0),
    ));
    commands.spawn((
        Name::new("Worker Lane"),
        Mesh2d(meshes.add(Rectangle::new(620.0, 300.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.08, 0.13, 0.12))),
        Transform::from_xyz(-300.0, -170.0, -5.0),
    ));
    commands.spawn((
        Name::new("Overlay Card"),
        Mesh2d(meshes.add(Rectangle::new(510.0, 760.0))),
        MeshMaterial2d(materials.add(Color::srgba(0.02, 0.03, 0.05, 0.88))),
        Transform::from_xyz(445.0, -6.0, -4.0),
    ));

    spawn_guard_scene(&mut commands, &mut meshes, &mut materials, guard_domain);
    spawn_worker_scene(&mut commands, &mut meshes, &mut materials, worker_domain);

    commands.spawn((
        Name::new("Lab Overlay"),
        LabOverlay,
        Text::new(String::new()),
        TextFont {
            font_size: 17.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: px(1060.0),
            top: px(58.0),
            width: px(420.0),
            ..default()
        },
    ));
}

fn configure_guard_domain(hooks: &mut GoapHooks, library: &mut GoapLibrary) -> saddle_ai_goap::GoapDomainId {
    let mut domain = saddle_ai_goap::GoapDomainDefinition::new("guard_replan");
    let has_target = domain.add_bool_key(
        "has_target",
        Some("any valid target is still available".into()),
        Some(false),
    );
    let neutralized = domain.add_bool_key(
        "neutralized",
        Some("the selected target has been neutralized".into()),
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
            .with_desired_state([saddle_ai_goap::FactCondition::equals_bool(neutralized, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "use target", "use_target")
            .with_preconditions([saddle_ai_goap::FactCondition::equals_bool(has_target, true)])
            .with_target("enemy", "guard_targets")
            .with_effects([saddle_ai_goap::FactEffect::set_bool(neutralized, true)]),
    );

    hooks.register_local_sensor("guard_sensor", move |world, _ctx| {
        let has_any_target = world
            .query_filtered::<Entity, With<GuardTarget>>()
            .iter(world)
            .next()
            .is_some();
        SensorOutput::new([saddle_ai_goap::FactPatch::set_bool(has_target, has_any_target)])
    });
    hooks.register_target_provider("guard_targets", |world, _ctx| {
        let mut query = world.query_filtered::<(Entity, &Transform, &Name), With<GuardTarget>>();
        let mut targets = query.iter(world).collect::<Vec<_>>();
        targets.sort_by(|(_, a, _), (_, b, _)| a.translation.x.total_cmp(&b.translation.x));
        targets
            .into_iter()
            .map(|(entity, transform, name)| {
                TargetCandidate::new(TargetToken(entity.to_bits()), name.as_str())
                    .with_debug_position(transform.translation)
            })
            .collect::<Vec<_>>()
    });

    library.register(domain)
}

fn configure_worker_domain(hooks: &mut GoapHooks, library: &mut GoapLibrary) -> saddle_ai_goap::GoapDomainId {
    let mut domain = saddle_ai_goap::GoapDomainDefinition::new("worker_cycle");
    let has_ore = domain.add_bool_key(
        "has_ore",
        Some("the worker carries ore".into()),
        Some(false),
    );
    let has_ingot = domain.add_bool_key(
        "has_ingot",
        Some("the worker carries a refined ingot".into()),
        Some(false),
    );
    let deposited = domain.add_bool_key(
        "deposited",
        Some("the worker has delivered the ingot".into()),
        Some(false),
    );
    let workbench_available = domain.add_bool_key(
        "workbench_available",
        Some("a workstation can accept ore right now".into()),
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

    hooks.register_local_sensor("worker_inventory", move |world, ctx| {
        let inventory = world
            .get::<WorkerInventory>(ctx.entity)
            .copied()
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

    library.register(domain)
}

fn spawn_guard_scene(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    domain: saddle_ai_goap::GoapDomainId,
) {
    commands.spawn((
        Name::new("Guard Header"),
        Text2d::new("Guard replan lane"),
        TextFont {
            font_size: 28.0,
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.89, 0.72)),
        Transform::from_xyz(-550.0, 270.0, 1.0),
    ));
    commands.spawn((
        Name::new(GUARD_NAME),
        GuardAgent,
        GoapAgent::new(domain),
        Mesh2d(meshes.add(Rectangle::new(74.0, 74.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.94, 0.61, 0.18))),
        Transform::from_xyz(-540.0, 150.0, 0.0),
    ));
    commands.spawn((
        Name::new(GUARD_TARGET_A_NAME),
        GuardTarget,
        Mesh2d(meshes.add(Rectangle::new(60.0, 60.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.86, 0.20, 0.28))),
        Transform::from_xyz(-180.0, 150.0, 0.0),
    ));
    commands.spawn((
        Name::new(GUARD_TARGET_B_NAME),
        GuardTarget,
        Mesh2d(meshes.add(Rectangle::new(60.0, 60.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.86, 0.20, 0.28))),
        Transform::from_xyz(-10.0, 150.0, 0.0),
    ));
}

fn spawn_worker_scene(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    domain: saddle_ai_goap::GoapDomainId,
) {
    commands.spawn((
        Name::new("Worker Header"),
        Text2d::new("Worker economy lane"),
        TextFont {
            font_size: 28.0,
            ..default()
        },
        TextColor(Color::srgb(0.78, 0.94, 0.84)),
        Transform::from_xyz(-550.0, -42.0, 1.0),
    ));
    commands.spawn((
        Name::new(WORKER_NAME),
        WorkerAgent,
        WorkerInventory::default(),
        GoapAgent::new(domain),
        Mesh2d(meshes.add(Rectangle::new(72.0, 72.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.22, 0.60, 0.88))),
        Transform::from_xyz(-540.0, -170.0, 0.0),
    ));
    for (name, x, color) in [
        ("Ore Node", -300.0, Color::srgb(0.54, 0.38, 0.28)),
        ("Workbench", -110.0, Color::srgb(0.64, 0.56, 0.24)),
        ("Depot", 80.0, Color::srgb(0.30, 0.76, 0.46)),
    ] {
        commands.spawn((
            Name::new(name),
            Mesh2d(meshes.add(Rectangle::new(118.0, 94.0))),
            MeshMaterial2d(materials.add(color)),
            Transform::from_xyz(x, -170.0, 0.0),
        ));
    }
}

fn remember_guard_dispatch(
    time: Res<Time>,
    mut dispatched: MessageReader<ActionDispatched>,
    guards: Query<(), With<GuardAgent>>,
    mut state: ResMut<GuardActionState>,
) {
    for message in dispatched.read() {
        if guards.get(message.entity).is_err() || message.executor.as_str() != "use_target" {
            continue;
        }
        state.active_entity = Some(message.entity);
        state.active_ticket = Some(message.ticket);
        state.active_target = message
            .target
            .as_ref()
            .and_then(|target| Entity::try_from_bits(target.token.0));
        state.started_at = time.elapsed_secs();
    }
}

fn remember_worker_dispatch(
    time: Res<Time>,
    mut dispatched: MessageReader<ActionDispatched>,
    workers: Query<(), With<WorkerAgent>>,
    mut state: ResMut<WorkerActionState>,
) {
    for message in dispatched.read() {
        if workers.get(message.entity).is_err() {
            continue;
        }
        state.active_entity = Some(message.entity);
        state.active_ticket = Some(message.ticket);
        state.active_executor = Some(message.executor.as_str().to_owned());
        state.started_at = time.elapsed_secs();
    }
}

fn advance_guard_actions(
    time: Res<Time>,
    mut state: ResMut<GuardActionState>,
    runtime: Query<&GoapRuntime, With<GuardAgent>>,
    mut reports: MessageWriter<ActionExecutionReport>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    let (Some(entity), Some(ticket)) = (state.active_entity, state.active_ticket) else {
        return;
    };

    let still_active = runtime
        .get(entity)
        .ok()
        .and_then(|runtime| runtime.active_action.as_ref())
        .is_some_and(|action| action.ticket == ticket);
    if !still_active {
        state.active_entity = None;
        state.active_ticket = None;
        state.active_target = None;
        return;
    }

    if now - state.started_at < GUARD_ACTION_DURATION {
        return;
    }

    if !state.removed_first_target {
        if let Some(target) = state.active_target.take() {
            commands.entity(target).despawn();
        }
        state.removed_first_target = true;
        state.active_entity = None;
        state.active_ticket = None;
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
    state.active_entity = None;
    state.active_ticket = None;
}

fn advance_worker_actions(
    time: Res<Time>,
    mut state: ResMut<WorkerActionState>,
    runtime: Query<&GoapRuntime, With<WorkerAgent>>,
    mut workers: Query<&mut WorkerInventory, With<WorkerAgent>>,
    mut workbench: ResMut<WorkbenchAvailability>,
    mut reports: MessageWriter<ActionExecutionReport>,
) {
    let now = time.elapsed_secs();
    if state
        .restore_workbench_at
        .is_some_and(|restore_at| now >= restore_at)
    {
        workbench.0 = true;
        state.restore_workbench_at = None;
    }

    let (Some(entity), Some(ticket), Some(executor)) = (
        state.active_entity,
        state.active_ticket,
        state.active_executor.clone(),
    ) else {
        return;
    };

    let still_active = runtime
        .get(entity)
        .ok()
        .and_then(|runtime| runtime.active_action.as_ref())
        .is_some_and(|action| action.ticket == ticket);
    if !still_active {
        state.active_entity = None;
        state.active_ticket = None;
        state.active_executor = None;
        return;
    }

    if now - state.started_at < WORKER_ACTION_DURATION {
        return;
    }

    let Ok(mut inventory) = workers.get_mut(entity) else {
        return;
    };

    match executor.as_str() {
        "gather_ore" => {
            inventory.has_ore = true;
            inventory.has_ingot = false;
            inventory.deposited = false;
            if !state.blocked_workbench_once {
                workbench.0 = false;
                state.restore_workbench_at = Some(now + WORKBENCH_RESTORE_DELAY);
                state.blocked_workbench_once = true;
            }
        }
        "smelt_ore" => {
            inventory.has_ore = false;
            inventory.has_ingot = true;
        }
        "deposit_ingot" => {
            inventory.has_ingot = false;
            inventory.deposited = true;
        }
        _ => {}
    }

    reports.write(ActionExecutionReport::new(
        entity,
        ticket,
        ActionExecutionStatus::Success,
    ));
    state.active_entity = None;
    state.active_ticket = None;
    state.active_executor = None;
}

fn record_goap_messages(
    mut started: MessageReader<saddle_ai_goap::PlanStarted>,
    mut invalidated: MessageReader<PlanInvalidated>,
    mut completed: MessageReader<PlanCompleted>,
    guards: Query<(), With<GuardAgent>>,
    workers: Query<(), With<WorkerAgent>>,
    mut diagnostics: ResMut<GoapLabDiagnostics>,
) {
    for message in started.read() {
        if guards.get(message.entity).is_ok() {
            diagnostics.guard_plan_starts += 1;
        }
        if workers.get(message.entity).is_ok() {
            diagnostics.worker_plan_starts += 1;
        }
    }

    for message in invalidated.read() {
        if guards.get(message.entity).is_ok() {
            diagnostics.guard_plan_invalidations += 1;
            diagnostics.guard_last_invalidation = Some(format!("{:?}", message.reason));
        }
        if workers.get(message.entity).is_ok() {
            diagnostics.worker_plan_invalidations += 1;
            diagnostics.worker_last_invalidation = Some(format!("{:?}", message.reason));
        }
    }

    for message in completed.read() {
        if guards.get(message.entity).is_ok() {
            diagnostics.guard_plan_completions += 1;
        }
        if workers.get(message.entity).is_ok() {
            diagnostics.worker_plan_completions += 1;
        }
    }
}

fn update_diagnostics(
    guard_snapshot: Single<&GoapDebugSnapshot, With<GuardAgent>>,
    worker_snapshot: Single<&GoapDebugSnapshot, With<WorkerAgent>>,
    worker_inventory: Single<&WorkerInventory, With<WorkerAgent>>,
    guard_targets: Query<Entity, With<GuardTarget>>,
    workbench: Res<WorkbenchAvailability>,
    scheduler: Res<GoapPlannerScheduler>,
    mut diagnostics: ResMut<GoapLabDiagnostics>,
) {
    diagnostics.guard_targets_remaining = guard_targets.iter().count();
    diagnostics.guard_status = guard_snapshot.planner_status.clone();
    diagnostics.worker_status = worker_snapshot.planner_status.clone();
    diagnostics.worker_has_ore = worker_inventory.has_ore;
    diagnostics.worker_has_ingot = worker_inventory.has_ingot;
    diagnostics.worker_deposited = worker_inventory.deposited;
    diagnostics.workbench_available = workbench.0;
    diagnostics.planner_queue_depth = scheduler.queue_depth;
}

fn update_overlay(
    diagnostics: Res<GoapLabDiagnostics>,
    guard_runtime: Single<(&GoapRuntime, &GoapDebugSnapshot), With<GuardAgent>>,
    worker_runtime: Single<(&GoapRuntime, &GoapDebugSnapshot), With<WorkerAgent>>,
    mut overlay: Single<&mut Text, With<LabOverlay>>,
) {
    overlay.0 = format!(
        "goap lab\n\
         left lane: target-aware replanning\n\
         right lane: multi-step worker loop\n\n\
         guard\n\
         goal: {}\n\
         status: {}\n\
         plan: {}\n\
         targets remaining: {}\n\
         plan starts / invalidations / completions: {} / {} / {}\n\
         last invalidation: {}\n\
         active target: {}\n\
         sensed: {}\n\n\
         worker\n\
         goal: {}\n\
         status: {}\n\
         plan: {}\n\
         workbench available: {}\n\
         inventory ore={} ingot={} deposited={}\n\
         plan starts / invalidations / completions: {} / {} / {}\n\
         last invalidation: {}\n\
         sensed: {}\n\n\
         planner queue depth: {}",
        goal_name(guard_runtime.0.current_goal.as_ref()),
        diagnostics.guard_status,
        format_plan(guard_runtime.1, guard_runtime.0.current_plan.as_ref()),
        diagnostics.guard_targets_remaining,
        diagnostics.guard_plan_starts,
        diagnostics.guard_plan_invalidations,
        diagnostics.guard_plan_completions,
        diagnostics
            .guard_last_invalidation
            .as_deref()
            .unwrap_or("none"),
        active_target_label(guard_runtime.0.current_plan.as_ref()),
        format_state(&guard_runtime.1.sensed_state),
        goal_name(worker_runtime.0.current_goal.as_ref()),
        diagnostics.worker_status,
        format_plan(worker_runtime.1, worker_runtime.0.current_plan.as_ref()),
        diagnostics.workbench_available,
        diagnostics.worker_has_ore,
        diagnostics.worker_has_ingot,
        diagnostics.worker_deposited,
        diagnostics.worker_plan_starts,
        diagnostics.worker_plan_invalidations,
        diagnostics.worker_plan_completions,
        diagnostics
            .worker_last_invalidation
            .as_deref()
            .unwrap_or("none"),
        format_state(&worker_runtime.1.sensed_state),
        diagnostics.planner_queue_depth,
    );
}

fn goal_name(goal: Option<&SelectedGoal>) -> &str {
    goal.map(|goal| goal.name.as_str()).unwrap_or("none")
}

fn format_plan(snapshot: &GoapDebugSnapshot, plan: Option<&GoapPlan>) -> String {
    if plan.is_none() && snapshot.plan_chain.is_empty() {
        return "none".into();
    }
    snapshot
        .plan_chain
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn active_target_label(plan: Option<&GoapPlan>) -> String {
    plan.and_then(GoapPlan::current_step)
        .and_then(|step| step.target.as_ref())
        .map(|target| target.label.clone())
        .unwrap_or_else(|| "none".into())
}

fn format_state(entries: &[saddle_ai_goap::GoapDebugEntry]) -> String {
    if entries.is_empty() {
        return "none".into();
    }
    entries
        .iter()
        .map(|entry| format!("{}={}", entry.key, entry.value))
        .collect::<Vec<_>>()
        .join(", ")
}
