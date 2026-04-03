use bevy::app::AppExit;
use bevy::prelude::*;
use saddle_ai_goap::{GoapAgent, GoapPlannerScheduler};
use saddle_pane::prelude::*;

#[derive(Resource, Clone, Copy)]
pub struct ExampleLifetime {
    pub duration_seconds: f32,
}

#[derive(Resource, Clone, Pane)]
#[pane(title = "GOAP Demo")]
pub struct GoapExamplePane {
    #[pane(slider, min = 0.1, max = 2.5, step = 0.05)]
    pub time_scale: f32,
    #[pane(slider, min = 0.0, max = 16.0, step = 1.0)]
    pub plan_cache_capacity: usize,
    #[pane(slider, min = 0.0, max = 1.0, step = 0.05)]
    pub goal_switch_margin: f32,
    #[pane(slider, min = 1.0, max = 128.0, step = 1.0)]
    pub max_agents_per_frame: usize,
    pub replan_on_sensed_state_change: bool,
}

impl Default for GoapExamplePane {
    fn default() -> Self {
        Self {
            time_scale: 1.0,
            plan_cache_capacity: 8,
            goal_switch_margin: 0.25,
            max_agents_per_frame: 8,
            replan_on_sensed_state_change: true,
        }
    }
}

pub fn pane_plugins() -> (
    bevy_flair::FlairPlugin,
    bevy_input_focus::InputDispatchPlugin,
    bevy_ui_widgets::UiWidgetsPlugins,
    bevy_input_focus::tab_navigation::TabNavigationPlugin,
    saddle_pane::PanePlugin,
) {
    (
        bevy_flair::FlairPlugin,
        bevy_input_focus::InputDispatchPlugin,
        bevy_ui_widgets::UiWidgetsPlugins,
        bevy_input_focus::tab_navigation::TabNavigationPlugin,
        saddle_pane::PanePlugin,
    )
}

pub fn configure_2d_example(app: &mut App, title: &str, duration_seconds: f32) {
    app.insert_resource(ClearColor(Color::srgb(0.05, 0.06, 0.08)));
    app.insert_resource(ExampleLifetime { duration_seconds });
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: title.into(),
            resolution: (1280, 720).into(),
            ..default()
        }),
        ..default()
    }));
    app.add_plugins(pane_plugins());
    app.register_pane::<GoapExamplePane>();
    app.add_systems(Startup, (spawn_backdrop, spawn_camera));
    app.add_systems(Update, (auto_exit, sync_pane_to_runtime));
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Name::new("Camera"), Camera2d));
}

fn spawn_backdrop(mut commands: Commands) {
    commands.spawn((
        Name::new("Backdrop"),
        Sprite::from_color(Color::srgb(0.07, 0.08, 0.11), Vec2::new(1600.0, 900.0)),
        Transform::from_xyz(0.0, 0.0, -20.0),
    ));
    commands.spawn((
        Name::new("Resource Band"),
        Sprite::from_color(Color::srgba(0.32, 0.22, 0.15, 0.22), Vec2::new(1160.0, 120.0)),
        Transform::from_xyz(0.0, -170.0, -10.0),
    ));
    commands.spawn((
        Name::new("Planning Band"),
        Sprite::from_color(Color::srgba(0.16, 0.36, 0.56, 0.18), Vec2::new(1160.0, 160.0)),
        Transform::from_xyz(0.0, 120.0, -10.0),
    ));
}

fn auto_exit(time: Res<Time>, lifetime: Res<ExampleLifetime>, mut exit: MessageWriter<AppExit>) {
    if time.elapsed_secs() >= lifetime.duration_seconds {
        exit.write(AppExit::Success);
    }
}

fn sync_pane_to_runtime(
    pane: Res<GoapExamplePane>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut scheduler: ResMut<GoapPlannerScheduler>,
    mut agents: Query<&mut GoapAgent>,
) {
    if !pane.is_changed() {
        return;
    }

    virtual_time.set_relative_speed(pane.time_scale.max(0.1));
    scheduler.max_agents_per_frame = pane.max_agents_per_frame.max(1);

    for mut agent in &mut agents {
        agent.config.plan_cache_capacity = pane.plan_cache_capacity;
        agent.config.goal_switch_margin = pane.goal_switch_margin.max(0.0);
        agent.config.replan_on_sensed_state_change = pane.replan_on_sensed_state_change;
    }
}
