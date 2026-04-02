use bevy::prelude::*;
use saddle_ai_goap::{
    ActionDefinition, ActionDispatched, ActionExecutionReport, ActionExecutionStatus, ActionId,
    GoalDefinition, GoalId, GoapAgent, GoapLibrary, GoapPlugin, GoapSystems, PlanCompleted,
};

#[derive(Component)]
struct BasicAgent;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "goap basic".into(),
                resolution: (900, 540).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(GoapPlugin::always_on(Update))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                execute_basic_action
                    .after(GoapSystems::Dispatch)
                    .before(GoapSystems::Monitor),
                tint_completed.after(GoapSystems::Monitor),
            ),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut library: ResMut<GoapLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((Name::new("Camera"), Camera2d));

    let mut domain = saddle_ai_goap::GoapDomainDefinition::new("basic_demo");
    let done = domain.add_bool_key(
        "done",
        Some("whether the setup task is complete".into()),
        Some(false),
    );
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish setup")
            .with_priority(10)
            .with_desired_state([saddle_ai_goap::FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "finish setup", "finish_setup")
            .with_effects([saddle_ai_goap::FactEffect::set_bool(done, true)]),
    );
    let domain_id = library.register(domain);

    commands.spawn((
        Name::new("Basic Agent"),
        BasicAgent,
        GoapAgent::new(domain_id),
        Mesh2d(meshes.add(Rectangle::new(110.0, 110.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.88, 0.46, 0.20))),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}

fn execute_basic_action(
    mut dispatched: MessageReader<ActionDispatched>,
    mut reports: MessageWriter<ActionExecutionReport>,
) {
    for message in dispatched.read() {
        if message.executor.as_str() == "finish_setup" {
            reports.write(ActionExecutionReport::new(
                message.entity,
                message.ticket,
                ActionExecutionStatus::Success,
            ));
        }
    }
}

fn tint_completed(
    mut completed: MessageReader<PlanCompleted>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    agent: Single<&MeshMaterial2d<ColorMaterial>, With<BasicAgent>>,
) {
    if completed.read().next().is_none() {
        return;
    }

    if let Some(material) = materials.get_mut(agent.0.id()) {
        material.color = Color::srgb(0.22, 0.78, 0.42);
    }
}
