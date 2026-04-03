use std::time::Duration;

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

use super::*;
use crate::{
    ActionCancelled, ActionDefinition, ActionDispatched, ActionExecutionReport,
    ActionExecutionStatus, ActionId, FactCondition, FactEffect, GoalChanged, GoalDefinition,
    GoalId, GoapAgent, GoapAgentConfig, GoapDomainDefinition, GoapHooks, GoapLibrary,
    GoapPlannerLimits, GoapPlannerScheduler, GoapPlugin, GoapRuntime, PlanCompleted, PlanFailed,
    PlanInvalidated, PlanInvalidationReason, SensorDefinition, SensorId, SensorInterval,
    SensorOutput, SensorScope, TargetToken,
};

#[derive(Resource, Default)]
struct BoolSensor(bool);

#[derive(Component, Default)]
struct StepProgress {
    prepared: bool,
}

#[derive(Resource, Default)]
struct GoalBias {
    attack: f32,
    rest: f32,
}

#[derive(Resource, Default)]
struct TargetPool(Vec<TargetCandidate>);

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct TestUpdate;

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct TestDeactivate;

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.insert_resource(Time::<()>::default());
    app.init_schedule(TestUpdate);
    app.init_schedule(TestDeactivate);
    app.add_plugins(GoapPlugin::new(TestUpdate, TestDeactivate, TestUpdate));
    app
}

fn run_test_schedule(app: &mut App) {
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_millis(16));
    app.world_mut().run_schedule(TestUpdate);
}

fn drain_messages<T: Message>(app: &mut App) -> Vec<T> {
    app.world_mut()
        .resource_mut::<Messages<T>>()
        .drain()
        .collect()
}

fn test_goal(name: &str) -> crate::SelectedGoal {
    crate::SelectedGoal {
        id: GoalId(99),
        name: name.into(),
        priority: 10,
        score: 10.0,
    }
}

fn test_action(
    id: usize,
    name: &str,
    cost: u32,
    preconditions: Vec<FactCondition>,
    effects: Vec<FactEffect>,
    sort_index: usize,
) -> crate::PreparedActionVariant {
    crate::PreparedActionVariant {
        action_id: ActionId(id),
        action_name: name.into(),
        executor: crate::HookKey::new(name),
        preconditions,
        effects,
        cost,
        target_slot: None,
        target: None,
        sort_index,
    }
}

fn plan_to_completion(mut session: crate::PlanningSession) -> crate::PlanningStepOutcome {
    loop {
        match session.step(64) {
            crate::PlanningStepOutcome::InProgress { .. } => continue,
            outcome => return outcome,
        }
    }
}

#[test]
fn custom_schedule_initializes_runtime_and_sensors() {
    let mut app = test_app();
    app.insert_resource(BoolSensor(true));

    let mut domain = GoapDomainDefinition::new("sense_only");
    let ready = domain.add_bool_key("ready", None::<String>, Some(false));
    domain.add_local_sensor(
        SensorDefinition::new(
            SensorId(0),
            "ready_sensor",
            SensorScope::Local,
            "ready_sensor",
            [ready],
        )
        .with_interval(SensorInterval::every(0.0)),
    );
    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    app.world_mut()
        .resource_mut::<GoapHooks>()
        .register_local_sensor("ready_sensor", move |world, _ctx| {
            SensorOutput::new([crate::world_state::FactPatch::set_bool(
                ready,
                world.resource::<BoolSensor>().0,
            )])
        });

    let entity = app
        .world_mut()
        .spawn((Name::new("Sense Agent"), GoapAgent::new(domain_id)))
        .id();

    run_test_schedule(&mut app);

    let runtime = app.world().get::<GoapRuntime>(entity).unwrap();
    assert!(runtime.sensed_state.get_bool(ready).unwrap());
}

#[test]
fn higher_scored_goal_is_selected() {
    let mut app = test_app();
    app.insert_resource(GoalBias {
        attack: 5.0,
        rest: 1.0,
    });

    let mut domain = GoapDomainDefinition::new("goal_select");
    let attack_done = domain.add_bool_key("attack_done", None::<String>, Some(false));
    let rest_done = domain.add_bool_key("rest_done", None::<String>, Some(false));
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "attack")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(attack_done, true)])
            .with_relevance("attack_score"),
    );
    domain.add_goal(
        GoalDefinition::new(GoalId(1), "rest")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(rest_done, true)])
            .with_relevance("rest_score"),
    );
    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    app.world_mut()
        .resource_mut::<GoapHooks>()
        .register_goal_score("attack_score", |world, _ctx| {
            world.resource::<GoalBias>().attack
        });
    app.world_mut()
        .resource_mut::<GoapHooks>()
        .register_goal_score("rest_score", |world, _ctx| {
            world.resource::<GoalBias>().rest
        });

    let entity = app
        .world_mut()
        .spawn((Name::new("Goal Agent"), GoapAgent::new(domain_id)))
        .id();

    run_test_schedule(&mut app);

    let runtime = app.world().get::<GoapRuntime>(entity).unwrap();
    assert_eq!(runtime.current_goal.as_ref().unwrap().name, "attack");
}

#[test]
fn action_success_feedback_completes_the_plan() {
    let mut app = test_app();

    let mut domain = GoapDomainDefinition::new("success_flow");
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "done")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "do_it", "exec")
            .with_effects([FactEffect::set_bool(done, true)]),
    );
    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    let entity = app
        .world_mut()
        .spawn((Name::new("Action Agent"), GoapAgent::new(domain_id)))
        .id();

    run_test_schedule(&mut app);
    let dispatched = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(dispatched.len(), 1);
    let dispatch = dispatched[0].clone();

    app.world_mut()
        .resource_mut::<Messages<ActionExecutionReport>>()
        .write(ActionExecutionReport::new(
            entity,
            dispatch.ticket,
            ActionExecutionStatus::Success,
        ));

    run_test_schedule(&mut app);
    let completed = drain_messages::<PlanCompleted>(&mut app);
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].goal.name, "done");

    let goal_changes = drain_messages::<GoalChanged>(&mut app);
    assert_eq!(goal_changes.len(), 2);
    let completion_clear = goal_changes
        .iter()
        .find(|message| message.new_goal.is_none())
        .expect("completion should clear the active goal");
    assert_eq!(
        completion_clear
            .previous_goal
            .as_ref()
            .map(|goal| goal.name.as_str()),
        Some("done")
    );
    assert!(completion_clear.new_goal.is_none());
}

#[test]
fn cached_plans_are_reused_for_identical_problems() {
    let goal_key = crate::WorldKeyId(0);
    let problem = crate::PlanningProblem {
        initial_state: crate::GoapWorldState::default(),
        state_revision: 7,
        goal: test_goal("cache"),
        desired_state: vec![crate::FactCondition::equals_bool(goal_key, true)],
        actions: vec![test_action(
            0,
            "finish",
            1,
            vec![],
            vec![crate::FactEffect::set_bool(goal_key, true)],
            0,
        )],
        limits: GoapPlannerLimits::default(),
    };

    let draft = match plan_to_completion(crate::PlanningSession::new(problem.clone())) {
        crate::PlanningStepOutcome::Success(plan) => plan,
        other => panic!("expected successful cached plan, got {other:?}"),
    };

    let mut runtime = GoapRuntime::new(Default::default(), Vec::new(), Vec::new(), 0);
    runtime.store_cached_plan(4, problem.clone(), draft.clone());

    let cached = runtime.cached_plan(&problem).expect("expected cached plan");
    assert_eq!(cached, draft);
    assert_eq!(runtime.plan_cache.len(), 1);
    assert_eq!(runtime.plan_cache[0].hit_count, 1);
}

#[test]
fn changing_a_required_fact_invalidates_and_requeues_the_plan() {
    let mut app = test_app();
    app.insert_resource(BoolSensor(true));

    let mut domain = GoapDomainDefinition::new("required_fact_change");
    let allow_finish = domain.add_bool_key("allow_finish", None::<String>, Some(true));
    let prepared = domain.add_bool_key("prepared", None::<String>, Some(false));
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_local_sensor(
        SensorDefinition::new(
            SensorId(0),
            "allow_sensor",
            SensorScope::Local,
            "allow_sensor",
            [allow_finish],
        )
        .with_interval(SensorInterval::every(0.0)),
    );
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "prepare", "prepare")
            .with_effects([FactEffect::set_bool(prepared, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(1), "finish", "finish")
            .with_preconditions([
                FactCondition::equals_bool(prepared, true),
                FactCondition::equals_bool(allow_finish, true),
            ])
            .with_effects([FactEffect::set_bool(done, true)]),
    );
    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    app.world_mut()
        .resource_mut::<GoapHooks>()
        .register_local_sensor("allow_sensor", move |world, _ctx| {
            SensorOutput::new([crate::world_state::FactPatch::set_bool(
                allow_finish,
                world.resource::<BoolSensor>().0,
            )])
        });

    let entity = app
        .world_mut()
        .spawn((Name::new("Prepared Agent"), GoapAgent::new(domain_id)))
        .id();

    run_test_schedule(&mut app);
    let dispatches = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(dispatches[0].action_name, "prepare");

    app.world_mut().resource_mut::<BoolSensor>().0 = false;
    app.world_mut()
        .resource_mut::<Messages<ActionExecutionReport>>()
        .write(ActionExecutionReport::new(
            entity,
            dispatches[0].ticket,
            ActionExecutionStatus::Success,
        ));

    run_test_schedule(&mut app);

    let invalidated = drain_messages::<PlanInvalidated>(&mut app);
    assert_eq!(invalidated.len(), 1);
    match &invalidated[0].reason {
        PlanInvalidationReason::RequiredFactChanged { key } => assert_eq!(*key, allow_finish),
        other => panic!("unexpected invalidation reason: {other:?}"),
    }
    assert_eq!(
        app.world().resource::<GoapPlannerScheduler>().queue_depth,
        1
    );
}

#[test]
fn target_loss_invalidates_running_target_action() {
    let mut app = test_app();
    app.insert_resource(TargetPool(vec![TargetCandidate::new(
        TargetToken(42),
        "Node A",
    )]));

    let mut domain = GoapDomainDefinition::new("target_loss");
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "interact")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "use_target", "use_target")
            .with_target("resource", "resource_targets")
            .with_effects([FactEffect::set_bool(done, true)]),
    );
    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    app.world_mut()
        .resource_mut::<GoapHooks>()
        .register_target_provider("resource_targets", |world, _ctx| {
            world.resource::<TargetPool>().0.clone()
        });

    let entity = app
        .world_mut()
        .spawn((Name::new("Target Agent"), GoapAgent::new(domain_id)))
        .id();

    run_test_schedule(&mut app);
    let dispatches = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(dispatches.len(), 1);

    app.world_mut().resource_mut::<TargetPool>().0.clear();
    run_test_schedule(&mut app);

    let invalidated = drain_messages::<PlanInvalidated>(&mut app);
    assert_eq!(invalidated.len(), 1);
    assert_eq!(
        invalidated[0].reason,
        PlanInvalidationReason::TargetInvalidated
    );
    assert_eq!(
        app.world().resource::<GoapPlannerScheduler>().queue_depth,
        1
    );
    assert!(
        app.world()
            .get::<GoapRuntime>(entity)
            .unwrap()
            .active_action
            .is_none()
    );
}

#[test]
fn context_validator_filters_invalid_targets_from_planning() {
    let mut app = test_app();
    app.insert_resource(TargetPool(vec![
        TargetCandidate::new(TargetToken(7), "Blocked"),
        TargetCandidate::new(TargetToken(42), "Reachable"),
    ]));

    let mut domain = GoapDomainDefinition::new("context_validator");
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "use reachable target")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "use target", "use_target")
            .with_target("resource", "resource_targets")
            .with_context_validator("reachable_only")
            .with_effects([FactEffect::set_bool(done, true)]),
    );
    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    app.world_mut()
        .resource_mut::<GoapHooks>()
        .register_target_provider("resource_targets", |world, _ctx| {
            world.resource::<TargetPool>().0.clone()
        });
    app.world_mut()
        .resource_mut::<GoapHooks>()
        .register_action_validator("reachable_only", |_world, ctx| {
            ctx.target
                .as_ref()
                .is_some_and(|target| target.token == TargetToken(42))
        });

    let entity = app
        .world_mut()
        .spawn((Name::new("Context Agent"), GoapAgent::new(domain_id)))
        .id();

    run_test_schedule(&mut app);

    let dispatched = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(dispatched.len(), 1);
    assert_eq!(dispatched[0].entity, entity);
    assert_eq!(
        dispatched[0].target.as_ref().map(|target| target.token),
        Some(TargetToken(42))
    );
}

#[test]
fn planner_scheduler_budget_limits_plans_per_frame() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<GoapPlannerScheduler>()
        .max_agents_per_frame = 1;

    let mut domain = GoapDomainDefinition::new("budget_queue");
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "finish", "finish")
            .with_effects([FactEffect::set_bool(done, true)]),
    );
    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);

    for index in 0..3 {
        app.world_mut().spawn((
            Name::new(format!("Budget Agent {}", index + 1)),
            GoapAgent::new(domain_id),
        ));
    }

    run_test_schedule(&mut app);

    let planned_agents = {
        let world = app.world_mut();
        world
            .query_filtered::<&GoapRuntime, With<GoapAgent>>()
            .iter(world)
            .filter(|runtime| runtime.current_plan.is_some())
            .count()
    };

    assert_eq!(planned_agents, 1);
    assert_eq!(
        app.world().resource::<GoapPlannerScheduler>().queue_depth,
        2
    );
}

#[test]
fn domain_default_planner_limits_apply_without_agent_override() {
    let mut app = test_app();

    let mut domain =
        GoapDomainDefinition::new("domain_default_limits").with_default_limits(GoapPlannerLimits {
            max_node_expansions: 32,
            max_plan_length: 1,
            max_expansions_per_step: 16,
        });
    let prepared = domain.add_bool_key("prepared", None::<String>, Some(false));
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "prepare", "prepare")
            .with_effects([FactEffect::set_bool(prepared, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(1), "finish", "finish")
            .with_preconditions([FactCondition::equals_bool(prepared, true)])
            .with_effects([FactEffect::set_bool(done, true)]),
    );

    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    let entity = app
        .world_mut()
        .spawn((Name::new("Domain Default Agent"), GoapAgent::new(domain_id)))
        .id();

    run_test_schedule(&mut app);

    let failures = drain_messages::<PlanFailed>(&mut app);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].reason.contains("plan-length guardrail"));
    assert!(
        app.world()
            .get::<GoapRuntime>(entity)
            .is_some_and(|runtime| runtime.current_plan.is_none())
    );
}

#[test]
fn agent_planner_limits_override_domain_defaults() {
    let mut app = test_app();

    let mut domain =
        GoapDomainDefinition::new("agent_override_limits").with_default_limits(GoapPlannerLimits {
            max_node_expansions: 32,
            max_plan_length: 1,
            max_expansions_per_step: 16,
        });
    let prepared = domain.add_bool_key("prepared", None::<String>, Some(false));
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "prepare", "prepare")
            .with_effects([FactEffect::set_bool(prepared, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(1), "finish", "finish")
            .with_preconditions([FactCondition::equals_bool(prepared, true)])
            .with_effects([FactEffect::set_bool(done, true)]),
    );

    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    let entity = app
        .world_mut()
        .spawn((
            Name::new("Override Agent"),
            GoapAgent::new(domain_id).with_config(GoapAgentConfig::default().with_planner_limits(
                GoapPlannerLimits {
                    max_node_expansions: 32,
                    max_plan_length: 2,
                    max_expansions_per_step: 16,
                },
            )),
        ))
        .id();

    run_test_schedule(&mut app);

    let dispatched = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(dispatched.len(), 1);
    assert_eq!(dispatched[0].entity, entity);
    assert_eq!(dispatched[0].action_name, "prepare");
}

#[test]
fn incremental_planning_counters_do_not_double_count_expansions() {
    let mut app = test_app();

    let mut domain =
        GoapDomainDefinition::new("incremental_counters").with_default_limits(GoapPlannerLimits {
            max_node_expansions: 32,
            max_plan_length: 4,
            max_expansions_per_step: 1,
        });
    let prepared = domain.add_bool_key("prepared", None::<String>, Some(false));
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "prepare", "prepare")
            .with_effects([FactEffect::set_bool(prepared, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(1), "finish", "finish")
            .with_preconditions([FactCondition::equals_bool(prepared, true)])
            .with_effects([FactEffect::set_bool(done, true)]),
    );

    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    let entity = app
        .world_mut()
        .spawn((Name::new("Incremental Agent"), GoapAgent::new(domain_id)))
        .id();

    for _ in 0..4 {
        run_test_schedule(&mut app);
    }

    let runtime = app
        .world()
        .get::<GoapRuntime>(entity)
        .expect("incremental agent should have a runtime");
    let plan = runtime
        .current_plan
        .as_ref()
        .expect("incremental agent should have a plan");

    assert_eq!(runtime.counters.last_expansions, plan.expansions);
    assert_eq!(
        runtime.counters.total_expansions,
        u64::from(plan.expansions)
    );
}

#[test]
fn failed_plan_waits_for_state_change_before_retrying() {
    let mut app = test_app();
    app.insert_resource(BoolSensor(false));

    let mut domain = GoapDomainDefinition::new("retry_gate");
    let ready = domain.add_bool_key("ready", None::<String>, Some(false));
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_local_sensor(
        SensorDefinition::new(
            SensorId(0),
            "ready_sensor",
            SensorScope::Local,
            "ready_sensor",
            [ready],
        )
        .with_interval(SensorInterval::every(0.0)),
    );
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "finish", "finish")
            .with_preconditions([FactCondition::equals_bool(ready, true)])
            .with_effects([FactEffect::set_bool(done, true)]),
    );

    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    app.world_mut()
        .resource_mut::<GoapHooks>()
        .register_local_sensor("ready_sensor", move |world, _ctx| {
            SensorOutput::new([crate::world_state::FactPatch::set_bool(
                ready,
                world.resource::<BoolSensor>().0,
            )])
        });

    let entity = app
        .world_mut()
        .spawn((Name::new("Retry Gate Agent"), GoapAgent::new(domain_id)))
        .id();

    run_test_schedule(&mut app);
    let first_failures = drain_messages::<PlanFailed>(&mut app);
    assert_eq!(first_failures.len(), 1);
    assert!(first_failures[0].reason.contains("no plan"));

    run_test_schedule(&mut app);
    assert!(drain_messages::<PlanFailed>(&mut app).is_empty());
    assert_eq!(
        app.world().resource::<GoapPlannerScheduler>().queue_depth,
        0
    );
    assert_eq!(
        app.world()
            .get::<GoapRuntime>(entity)
            .map(|runtime| runtime.status.clone()),
        Some(PlannerStatus::Failed)
    );

    app.world_mut().resource_mut::<BoolSensor>().0 = true;
    run_test_schedule(&mut app);

    let dispatched = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(dispatched.len(), 1);
    assert_eq!(dispatched[0].entity, entity);
    assert_eq!(dispatched[0].action_name, "finish");
}

#[test]
fn sensor_refresh_invalidates_stale_plan_when_policy_enabled() {
    let mut app = test_app();
    app.insert_resource(BoolSensor(false));

    let mut domain = GoapDomainDefinition::new("sensor_refresh_enabled");
    let route_clear = domain.add_bool_key("route_clear", None::<String>, Some(false));
    let prepared = domain.add_bool_key("prepared", None::<String>, Some(false));
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_local_sensor(
        SensorDefinition::new(
            SensorId(0),
            "route_sensor",
            SensorScope::Local,
            "route_sensor",
            [route_clear, prepared],
        )
        .with_interval(SensorInterval::every(0.0)),
    );
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "prepare", "prepare")
            .with_effects([FactEffect::set_bool(prepared, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(1), "safe_finish", "safe_finish")
            .with_preconditions([FactCondition::equals_bool(prepared, true)])
            .with_base_cost(5)
            .with_effects([FactEffect::set_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(2), "fast_finish", "fast_finish")
            .with_preconditions([
                FactCondition::equals_bool(prepared, true),
                FactCondition::equals_bool(route_clear, true),
            ])
            .with_base_cost(1)
            .with_effects([FactEffect::set_bool(done, true)]),
    );
    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    app.world_mut()
        .resource_mut::<GoapHooks>()
        .register_local_sensor("route_sensor", move |world, _ctx| {
            let progress = world
                .get::<StepProgress>(_ctx.entity)
                .map(|progress| progress.prepared)
                .unwrap_or(false);
            SensorOutput::new([
                crate::world_state::FactPatch::set_bool(
                    route_clear,
                    world.resource::<BoolSensor>().0,
                ),
                crate::world_state::FactPatch::set_bool(prepared, progress),
            ])
        });

    let entity = app
        .world_mut()
        .spawn((
            Name::new("Sensor Agent"),
            StepProgress::default(),
            GoapAgent::new(domain_id),
        ))
        .id();

    run_test_schedule(&mut app);
    let dispatches = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0].action_name, "prepare");
    app.world_mut()
        .entity_mut(entity)
        .insert(StepProgress { prepared: true });

    app.world_mut()
        .resource_mut::<Messages<ActionExecutionReport>>()
        .write(ActionExecutionReport::new(
            entity,
            dispatches[0].ticket,
            ActionExecutionStatus::Success,
        ));
    app.world_mut().resource_mut::<BoolSensor>().0 = true;

    run_test_schedule(&mut app);

    run_test_schedule(&mut app);
    let invalidated = drain_messages::<PlanInvalidated>(&mut app);
    assert_eq!(invalidated.len(), 1);
    assert_eq!(invalidated[0].reason, PlanInvalidationReason::SensorRefresh);

    let next_dispatches = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(next_dispatches.len(), 1);
    assert_eq!(next_dispatches[0].action_name, "fast_finish");
}

#[test]
fn sensor_refresh_keeps_plan_when_policy_disabled() {
    let mut app = test_app();
    app.insert_resource(BoolSensor(false));

    let mut domain = GoapDomainDefinition::new("sensor_refresh_disabled");
    let route_clear = domain.add_bool_key("route_clear", None::<String>, Some(false));
    let prepared = domain.add_bool_key("prepared", None::<String>, Some(false));
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_local_sensor(
        SensorDefinition::new(
            SensorId(0),
            "route_sensor",
            SensorScope::Local,
            "route_sensor",
            [route_clear, prepared],
        )
        .with_interval(SensorInterval::every(0.0)),
    );
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "prepare", "prepare")
            .with_effects([FactEffect::set_bool(prepared, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(1), "safe_finish", "safe_finish")
            .with_preconditions([FactCondition::equals_bool(prepared, true)])
            .with_base_cost(5)
            .with_effects([FactEffect::set_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(2), "fast_finish", "fast_finish")
            .with_preconditions([
                FactCondition::equals_bool(prepared, true),
                FactCondition::equals_bool(route_clear, true),
            ])
            .with_base_cost(1)
            .with_effects([FactEffect::set_bool(done, true)]),
    );
    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);
    app.world_mut()
        .resource_mut::<GoapHooks>()
        .register_local_sensor("route_sensor", move |world, _ctx| {
            let progress = world
                .get::<StepProgress>(_ctx.entity)
                .map(|progress| progress.prepared)
                .unwrap_or(false);
            SensorOutput::new([
                crate::world_state::FactPatch::set_bool(
                    route_clear,
                    world.resource::<BoolSensor>().0,
                ),
                crate::world_state::FactPatch::set_bool(prepared, progress),
            ])
        });

    let entity = app.world_mut().spawn((
        Name::new("Stable Agent"),
        StepProgress::default(),
        GoapAgent::new(domain_id).with_config(GoapAgentConfig {
            replan_on_sensed_state_change: false,
            ..Default::default()
        }),
    ));
    let entity = entity.id();

    run_test_schedule(&mut app);
    let dispatches = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(dispatches.len(), 1);
    app.world_mut()
        .entity_mut(entity)
        .insert(StepProgress { prepared: true });

    app.world_mut()
        .resource_mut::<Messages<ActionExecutionReport>>()
        .write(ActionExecutionReport::new(
            entity,
            dispatches[0].ticket,
            ActionExecutionStatus::Success,
        ));
    app.world_mut().resource_mut::<BoolSensor>().0 = true;

    run_test_schedule(&mut app);

    let invalidated = drain_messages::<PlanInvalidated>(&mut app);
    assert!(invalidated.is_empty());

    run_test_schedule(&mut app);
    let next_dispatches = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(next_dispatches.len(), 1);
    assert_eq!(next_dispatches[0].action_name, "safe_finish");
}

#[test]
fn cancelled_action_report_emits_single_cancellation_message() {
    let mut app = test_app();

    let mut domain = GoapDomainDefinition::new("cancel_flow");
    let done = domain.add_bool_key("done", None::<String>, Some(false));
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "wait", "wait")
            .with_effects([FactEffect::set_bool(done, true)]),
    );
    let domain_id = app
        .world_mut()
        .resource_mut::<GoapLibrary>()
        .register(domain);

    let entity = app
        .world_mut()
        .spawn((Name::new("Cancel Agent"), GoapAgent::new(domain_id)))
        .id();

    run_test_schedule(&mut app);
    let dispatches = drain_messages::<ActionDispatched>(&mut app);
    assert_eq!(dispatches.len(), 1);

    app.world_mut()
        .resource_mut::<Messages<ActionExecutionReport>>()
        .write(ActionExecutionReport::new(
            entity,
            dispatches[0].ticket,
            ActionExecutionStatus::Cancelled {
                reason: "interrupt".into(),
            },
        ));

    run_test_schedule(&mut app);

    let invalidated = drain_messages::<PlanInvalidated>(&mut app);
    assert_eq!(invalidated.len(), 1);
    assert_eq!(
        invalidated[0].reason,
        PlanInvalidationReason::Manual {
            reason: "interrupt".into()
        }
    );

    let cancelled = drain_messages::<ActionCancelled>(&mut app);
    assert_eq!(cancelled.len(), 1);
    assert_eq!(
        cancelled[0].reason,
        PlanInvalidationReason::Manual {
            reason: "interrupt".into()
        }
    );
}
