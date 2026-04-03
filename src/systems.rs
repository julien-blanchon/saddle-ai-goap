use std::collections::HashSet;

use bevy::prelude::*;

use crate::components::{
    ActiveAction, ActiveActionStatus, GoapAgent, GoapPlan, GoapRuntime, PlanInvalidationReason,
    PlannerStatus, SensorRuntimeInfo,
};
use crate::debug::{GoapDebugEntry, GoapDebugSnapshot};
use crate::definitions::{ActionDefinition, GoalDefinition, GoapDomainDefinition, GoapDomainId};
use crate::execution::{
    ActionEvaluationContext, GlobalSensorContext, GoalHookContext, LocalSensorContext,
    SensorOutput, TargetProviderContext,
};
use crate::messages::{
    ActionCancelled, ActionDispatched, ActionExecutionReport, ActionExecutionStatus, GoalChanged,
    InvalidateGlobalSensors, InvalidateGoapAgent, InvalidateLocalSensors, PlanCompleted,
    PlanFailed, PlanInvalidated, PlanStarted,
};
use crate::planner::{
    GoapPlanDraft, PlanningFailureReason, PlanningProblem, PlanningSession, PlanningStepOutcome,
    PreparedActionVariant, SelectedGoal, TargetCandidate,
};
use crate::resources::{
    GoapGlobalSensorCache, GoapHooks, GoapLibrary, GoapMessageCursors, GoapPlannerScheduler,
};
use crate::world_state::GoapWorldState;

pub(crate) fn activate_agents(world: &mut World) {
    initialize_missing_agents(world);
}

pub(crate) fn deactivate_agents(world: &mut World) {
    let entities = world
        .query_filtered::<Entity, With<GoapRuntime>>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in entities {
        world.resource_mut::<GoapPlannerScheduler>().remove(entity);
        if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
            entity_mut.remove::<GoapRuntime>();
            entity_mut.remove::<GoapDebugSnapshot>();
        }
    }
}

pub(crate) fn sense_agents(world: &mut World) {
    initialize_missing_agents(world);

    let now = time_seconds(world);
    let entities = agent_entities(world);

    let invalidate_local = collect_local_invalidations(world);
    let invalidate_global = collect_global_invalidations(world);

    let mut active_domains = HashSet::<GoapDomainId>::default();
    for entity in &entities {
        if let Some(agent) = world.get::<GoapAgent>(*entity) {
            active_domains.insert(agent.domain);
        }
    }

    for domain_id in active_domains {
        refresh_global_sensors(
            world,
            domain_id,
            now,
            invalidate_global.contains(&domain_id),
        );
    }

    for entity in entities {
        refresh_local_sensors(world, entity, now, invalidate_local.contains(&entity));
    }
}

pub(crate) fn select_goals(world: &mut World) {
    let entities = agent_entities(world);
    for entity in entities {
        let Some(agent) = world.get::<GoapAgent>(entity).cloned() else {
            continue;
        };
        let Some(domain) = clone_domain(world, agent.domain) else {
            continue;
        };
        let Some((
            state,
            current_goal,
            has_plan,
            active_action,
            status,
            last_failed_goal,
            last_failed_revision,
            current_plan_revision,
            planning_session_revision,
            sensor_revision,
        )) = runtime_snapshot(world, &domain, entity)
        else {
            continue;
        };

        let best_goal = evaluate_best_goal(
            world,
            entity,
            agent.domain,
            &domain,
            &state,
            current_goal.as_ref(),
        );

        let current_goal_completed = current_goal.as_ref().is_some_and(|selected| {
            domain.goals.get(selected.id.0).is_some_and(|definition| {
                goal_completed(
                    world,
                    entity,
                    agent.domain,
                    definition,
                    &state,
                    current_goal.as_ref(),
                )
            })
        });
        let current_goal_valid = current_goal.as_ref().is_none_or(|selected| {
            domain.goals.get(selected.id.0).is_some_and(|definition| {
                goal_valid(
                    world,
                    entity,
                    agent.domain,
                    definition,
                    &state,
                    current_goal.as_ref(),
                )
            })
        });

        if current_goal_completed {
            finish_goal(world, entity);
        } else if !current_goal_valid {
            invalidate_plan(
                world,
                entity,
                PlanInvalidationReason::GoalNoLongerValid,
                false,
            );
            set_goal(world, entity, None);
        }

        let Some(best_goal) = best_goal else {
            if current_goal.is_some() {
                set_goal(world, entity, None);
            }
            continue;
        };

        let should_switch = match current_goal.as_ref() {
            None => true,
            Some(current) if current.id == best_goal.id => false,
            Some(current) => {
                agent.config.preempt_on_better_goal
                    && best_goal.score > current.score + agent.config.goal_switch_margin
            }
        };

        if should_switch {
            if current_goal.is_some() {
                invalidate_plan(
                    world,
                    entity,
                    PlanInvalidationReason::HigherPriorityGoal,
                    false,
                );
            }
            set_goal(world, entity, Some(best_goal.clone()));
            request_plan(world, entity);
            if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
                runtime.status = PlannerStatus::QueuedForPlanning;
                runtime.counters.goal_switches += 1;
            }
            continue;
        }

        let stale_from_sensor_refresh = agent.config.replan_on_sensed_state_change
            && active_action.is_none()
            && current_goal.is_some()
            && (current_plan_revision.is_some_and(|revision| revision < sensor_revision)
                || planning_session_revision.is_some_and(|revision| revision < sensor_revision));

        if stale_from_sensor_refresh {
            invalidate_plan(world, entity, PlanInvalidationReason::SensorRefresh, true);
            continue;
        }

        if best_goal.id
            == current_goal
                .as_ref()
                .map(|goal| goal.id)
                .unwrap_or(best_goal.id)
            && status != PlannerStatus::Planning
            && !has_plan
            && active_action.is_none()
            && !matches!(
                (status.clone(), current_goal.as_ref(), last_failed_goal, last_failed_revision),
                (
                    PlannerStatus::Failed,
                    Some(current_goal),
                    Some(failed_goal),
                    Some(failed_revision),
                ) if current_goal.id == failed_goal && failed_revision == sensor_revision
            )
        {
            request_plan(world, entity);
        }
    }
}

pub(crate) fn advance_planning(world: &mut World) {
    let max_agents = world
        .resource::<GoapPlannerScheduler>()
        .max_agents_per_frame;

    for _ in 0..max_agents {
        let Some(entity) = world.resource_mut::<GoapPlannerScheduler>().dequeue() else {
            break;
        };

        let Some(agent) = world.get::<GoapAgent>(entity).cloned() else {
            continue;
        };
        let Some(domain) = clone_domain(world, agent.domain) else {
            continue;
        };
        let limits = agent
            .config
            .resolve_planner_limits(domain.default_planner_limits);

        let start_session = world
            .get::<GoapRuntime>(entity)
            .is_some_and(|runtime| runtime.planning_session.is_none());
        if start_session {
            let Some(problem) = build_planning_problem(world, entity, &agent, &domain) else {
                emit_message(
                    world,
                    PlanFailed {
                        entity,
                        goal: world
                            .get::<GoapRuntime>(entity)
                            .and_then(|runtime| runtime.current_goal.clone()),
                        status: PlannerStatus::Failed,
                        reason: "no planning problem available".into(),
                    },
                );
                if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
                    runtime.status = PlannerStatus::Failed;
                    runtime.counters.failed_plans += 1;
                }
                continue;
            };
            let cached_plan = world
                .get_mut::<GoapRuntime>(entity)
                .and_then(|mut runtime| runtime.cached_plan(&problem));
            if let Some(draft) = cached_plan {
                if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
                    runtime.status = PlannerStatus::Dispatching;
                    runtime.counters.cached_plan_hits += 1;
                }
                apply_successful_plan(world, entity, draft);
                continue;
            }
            if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
                runtime.status = PlannerStatus::Planning;
                runtime.planning_session = Some(PlanningSession::new(problem));
            }
        }

        let Some(mut session) = world
            .get_mut::<GoapRuntime>(entity)
            .and_then(|mut runtime| runtime.planning_session.take())
        else {
            continue;
        };

        let outcome = session.step(limits.max_expansions_per_step);
        match outcome {
            PlanningStepOutcome::InProgress {
                total_expansions, ..
            } => {
                if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
                    runtime.status = PlannerStatus::Planning;
                    runtime.counters.last_expansions = total_expansions;
                    runtime.planning_session = Some(session);
                }
                if total_expansions < limits.max_node_expansions {
                    request_plan(world, entity);
                }
            }
            PlanningStepOutcome::Success(draft) => {
                let session_problem = session.problem().clone();
                if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
                    runtime.store_cached_plan(
                        agent.config.plan_cache_capacity,
                        session_problem,
                        draft.clone(),
                    );
                }
                apply_successful_plan(world, entity, draft);
            }
            PlanningStepOutcome::Failure {
                reason,
                total_expansions,
                ..
            } => {
                let reason_text = match reason {
                    PlanningFailureReason::NoPlan => "no plan could satisfy the goal",
                    PlanningFailureReason::MaxNodeExpansions => {
                        "planner hit the node-expansion guardrail"
                    }
                    PlanningFailureReason::MaxPlanLength => "planner hit the plan-length guardrail",
                };
                let goal = world
                    .get::<GoapRuntime>(entity)
                    .and_then(|runtime| runtime.current_goal.clone());
                emit_message(
                    world,
                    PlanFailed {
                        entity,
                        goal,
                        status: PlannerStatus::Failed,
                        reason: reason_text.into(),
                    },
                );
                if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
                    runtime.status = PlannerStatus::Failed;
                    runtime.counters.failed_plans += 1;
                    runtime.counters.last_expansions = total_expansions;
                    runtime.counters.total_expansions += u64::from(total_expansions);
                    runtime.last_failed_goal = runtime.current_goal.as_ref().map(|goal| goal.id);
                    runtime.last_failed_revision = Some(runtime.sensor_revision);
                    runtime.planning_session = None;
                    runtime.current_plan = None;
                }
            }
        }
    }
}

pub(crate) fn dispatch_actions(world: &mut World) {
    let now = time_seconds(world);
    for entity in agent_entities(world) {
        let Some((goal, step, ticket)) = ({
            let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) else {
                continue;
            };

            if runtime.active_action.is_some() {
                continue;
            }

            if let Some(plan) = runtime.current_plan.as_ref() {
                if plan.finished() {
                    continue;
                }
                let step = plan.current_step().cloned();
                let goal = plan.goal.clone();
                let ticket = runtime.next_action_ticket;
                runtime.next_action_ticket += 1;
                if let Some(step) = step.as_ref() {
                    runtime.active_action = Some(ActiveAction::from_step(ticket, step, now));
                    runtime.status = PlannerStatus::WaitingOnAction;
                    runtime.counters.dispatched_actions += 1;
                }
                step.map(|step| (goal, step, ticket))
            } else {
                None
            }
        }) else {
            continue;
        };

        emit_message(
            world,
            ActionDispatched {
                entity,
                goal_id: goal.id,
                action_id: step.action_id,
                action_name: step.action_name.clone(),
                executor: step.executor.clone(),
                ticket,
                target_slot: step.target_slot.clone(),
                target: step.target.clone(),
            },
        );
    }
}

pub(crate) fn monitor_actions(world: &mut World) {
    let reports = collect_action_reports(world);
    let manual_invalidations = collect_agent_invalidations(world);

    for invalidation in manual_invalidations {
        invalidate_plan(world, invalidation.entity, invalidation.reason, true);
    }

    for report in reports {
        handle_action_report(world, report);
    }

    for entity in agent_entities(world) {
        let Some(agent) = world.get::<GoapAgent>(entity).cloned() else {
            continue;
        };
        let Some(domain) = clone_domain(world, agent.domain) else {
            continue;
        };
        let Some((state, current_goal, current_plan, active_action)) =
            runtime_monitor_snapshot(world, &domain, entity)
        else {
            continue;
        };

        let Some(goal) = current_goal else {
            continue;
        };

        if goal_completed(
            world,
            entity,
            agent.domain,
            &domain.goals[goal.id.0],
            &state,
            Some(&goal),
        ) {
            finish_goal(world, entity);
            continue;
        }

        let Some(plan) = current_plan else {
            continue;
        };
        let Some(step) = plan.current_step() else {
            finish_goal(world, entity);
            continue;
        };

        if let Some(reason) = first_failed_precondition(&domain, &plan, &state) {
            invalidate_plan(world, entity, reason, true);
            continue;
        }

        if active_action.is_some()
            && !step_target_is_still_valid(world, entity, agent.domain, &domain, &goal, step)
        {
            invalidate_plan(
                world,
                entity,
                PlanInvalidationReason::TargetInvalidated,
                true,
            );
        }
    }
}

pub(crate) fn cleanup_agents(world: &mut World) {
    let orphaned = world
        .query_filtered::<Entity, (With<GoapRuntime>, Without<GoapAgent>)>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in orphaned {
        world.resource_mut::<GoapPlannerScheduler>().remove(entity);
        if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
            entity_mut.remove::<GoapRuntime>();
            entity_mut.remove::<GoapDebugSnapshot>();
        }
    }
}

pub(crate) fn refresh_debug_snapshots(world: &mut World) {
    let entities = world
        .query_filtered::<Entity, (With<GoapAgent>, With<GoapRuntime>, With<GoapDebugSnapshot>)>()
        .iter(world)
        .collect::<Vec<_>>();

    for entity in entities {
        let Some(agent) = world.get::<GoapAgent>(entity).cloned() else {
            continue;
        };
        let Some(domain) = clone_domain(world, agent.domain) else {
            continue;
        };
        let Some((status, goal, plan, invalidation, counters, state, active_action)) =
            runtime_debug_snapshot(world, &domain, entity)
        else {
            continue;
        };

        let plan_chain = plan
            .as_ref()
            .map(|plan| {
                plan.steps
                    .iter()
                    .enumerate()
                    .map(|(index, step)| {
                        let prefix = if index == plan.cursor { ">" } else { " " };
                        match (&step.target_slot, &step.target) {
                            (Some(slot), Some(target)) => {
                                format!("{prefix} {} [{}:{}]", step.action_name, slot, target.label)
                            }
                            _ => format!("{prefix} {}", step.action_name),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let active_targets = active_action
            .as_ref()
            .and_then(|action| action.target.as_ref().map(|target| (action, target)))
            .map(|(action, target)| {
                vec![format!(
                    "{} => {}",
                    action
                        .target_slot
                        .clone()
                        .unwrap_or_else(|| "target".into()),
                    target.label
                )]
            })
            .unwrap_or_default();

        let sensed_state = state
            .describe(&domain.schema)
            .into_iter()
            .map(|(key, value)| GoapDebugEntry { key, value })
            .collect::<Vec<_>>();

        let counters = vec![
            GoapDebugEntry {
                key: "sensor_refreshes".into(),
                value: counters.sensor_refreshes.to_string(),
            },
            GoapDebugEntry {
                key: "replans".into(),
                value: counters.replans.to_string(),
            },
            GoapDebugEntry {
                key: "invalidations".into(),
                value: counters.invalidations.to_string(),
            },
            GoapDebugEntry {
                key: "dispatched_actions".into(),
                value: counters.dispatched_actions.to_string(),
            },
            GoapDebugEntry {
                key: "completed_plans".into(),
                value: counters.completed_plans.to_string(),
            },
            GoapDebugEntry {
                key: "failed_plans".into(),
                value: counters.failed_plans.to_string(),
            },
            GoapDebugEntry {
                key: "goal_switches".into(),
                value: counters.goal_switches.to_string(),
            },
            GoapDebugEntry {
                key: "last_expansions".into(),
                value: counters.last_expansions.to_string(),
            },
        ];

        if let Some(mut snapshot) = world.get_mut::<GoapDebugSnapshot>(entity) {
            snapshot.current_goal = goal.as_ref().map(|goal| goal.name.clone());
            snapshot.planner_status = format!("{status:?}");
            snapshot.plan_chain = plan_chain;
            snapshot.active_targets = active_targets;
            snapshot.last_invalidation = invalidation.map(|reason| format!("{reason:?}"));
            snapshot.sensed_state = sensed_state;
            snapshot.counters = counters;
        }
    }
}

fn initialize_missing_agents(world: &mut World) {
    let entities = world
        .query_filtered::<Entity, (With<GoapAgent>, Without<GoapRuntime>)>()
        .iter(world)
        .collect::<Vec<_>>();
    for entity in entities {
        let Some(agent) = world.get::<GoapAgent>(entity).cloned() else {
            continue;
        };
        let Some(domain) = clone_domain(world, agent.domain) else {
            continue;
        };

        let global_sensors = domain
            .global_sensors
            .iter()
            .map(SensorRuntimeInfo::from_definition)
            .collect::<Vec<_>>();
        let default_state = domain.default_state();
        let global_revision = {
            let mut cache = world.resource_mut::<GoapGlobalSensorCache>();
            cache
                .ensure_domain(agent.domain, default_state.clone(), global_sensors.clone())
                .revision
        };
        let global_state = world
            .resource::<GoapGlobalSensorCache>()
            .get(agent.domain)
            .map(|cache| cache.state.clone())
            .unwrap_or(default_state.clone());

        let mut sensed_state = default_state;
        sensed_state.overlay(&global_state);

        let local_sensors = domain
            .local_sensors
            .iter()
            .map(SensorRuntimeInfo::from_definition)
            .collect::<Vec<_>>();

        if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
            entity_mut.insert(GoapRuntime::new(
                sensed_state,
                local_sensors,
                global_sensors,
                global_revision,
            ));
            entity_mut.insert(GoapDebugSnapshot::default());
        }
    }
}

fn refresh_global_sensors(world: &mut World, domain_id: GoapDomainId, now: f32, force: bool) {
    let Some(domain) = clone_domain(world, domain_id) else {
        return;
    };

    let mut changed = false;
    for (index, definition) in domain.global_sensors.iter().enumerate() {
        let should_run = {
            let cache = world.resource::<GoapGlobalSensorCache>();
            cache
                .get(domain_id)
                .and_then(|cache| cache.sensors.get(index))
                .is_some_and(|sensor| force || now >= sensor.next_due_seconds)
        };
        if !should_run {
            continue;
        }

        let Some(handler) = world
            .resource::<GoapHooks>()
            .global_sensor(&definition.handler)
            .cloned()
        else {
            continue;
        };

        let current_state = world
            .resource::<GoapGlobalSensorCache>()
            .get(domain_id)
            .map(|cache| cache.state.clone())
            .unwrap_or_else(|| domain.default_state());

        let output = handler(
            world,
            GlobalSensorContext {
                domain_id,
                current_state,
            },
        );

        if let Some(cache) = world
            .resource_mut::<GoapGlobalSensorCache>()
            .get_mut(domain_id)
        {
            changed |= apply_sensor_output(&mut cache.state, &output);
            if let Some(sensor) = cache.sensors.get_mut(index) {
                sensor.last_run_seconds = Some(now);
                sensor.next_due_seconds = now + definition.interval.seconds;
                sensor.run_count += 1;
                sensor.last_note = output.note.clone();
            }
        }
    }

    if changed {
        if let Some(cache) = world
            .resource_mut::<GoapGlobalSensorCache>()
            .get_mut(domain_id)
        {
            cache.revision += 1;
        }
    }
}

fn refresh_local_sensors(world: &mut World, entity: Entity, now: f32, force: bool) {
    let Some(agent) = world.get::<GoapAgent>(entity).cloned() else {
        return;
    };
    let Some(domain) = clone_domain(world, agent.domain) else {
        return;
    };
    let Some((
        local_sensor_state,
        local_state_before,
        sensed_state_before,
        observed_global_revision,
    )) = world.get::<GoapRuntime>(entity).map(|runtime| {
        (
            runtime.local_sensors.clone(),
            runtime.local_state.clone(),
            runtime.sensed_state.clone(),
            runtime.observed_global_revision,
        )
    })
    else {
        return;
    };

    let global_cache = world
        .resource::<GoapGlobalSensorCache>()
        .get(agent.domain)
        .cloned()
        .unwrap_or_else(|| crate::resources::DomainGlobalCache {
            state: domain.default_state(),
            revision: 0,
            sensors: Vec::new(),
        });
    let global_changed = global_cache.revision != observed_global_revision;
    let mut local_state = local_state_before;
    let mut total_refreshes = 0_u64;
    let mut any_patch_change = false;

    for (index, definition) in domain.local_sensors.iter().enumerate() {
        let should_run = local_sensor_state
            .get(index)
            .is_some_and(|sensor| force || global_changed || now >= sensor.next_due_seconds);
        if !should_run {
            continue;
        }

        let Some(handler) = world
            .resource::<GoapHooks>()
            .local_sensor(&definition.handler)
            .cloned()
        else {
            continue;
        };

        let output = handler(
            world,
            LocalSensorContext {
                entity,
                domain_id: agent.domain,
                current_state: sensed_state_before.clone(),
                global_state: global_cache.state.clone(),
            },
        );
        any_patch_change |= apply_sensor_output(&mut local_state, &output);
        if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
            if let Some(sensor) = runtime.local_sensors.get_mut(index) {
                sensor.last_run_seconds = Some(now);
                sensor.next_due_seconds = now + definition.interval.seconds;
                sensor.run_count += 1;
                sensor.last_note = output.note.clone();
            }
        }
        total_refreshes += 1;
    }

    let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) else {
        return;
    };

    let mut combined = domain.default_state();
    combined.overlay(&global_cache.state);
    combined.overlay(&local_state);

    runtime.global_sensors = global_cache.sensors.clone();
    runtime.observed_global_revision = global_cache.revision;

    if any_patch_change || global_changed || combined != runtime.sensed_state {
        runtime.sensed_state = combined;
        runtime.local_state = local_state;
        runtime.sensor_revision += 1;
        runtime.status = PlannerStatus::SelectingGoal;
    }

    runtime.counters.sensor_refreshes += total_refreshes;
}

fn evaluate_best_goal(
    world: &mut World,
    entity: Entity,
    domain_id: GoapDomainId,
    domain: &GoapDomainDefinition,
    state: &GoapWorldState,
    current_goal: Option<&SelectedGoal>,
) -> Option<SelectedGoal> {
    let mut best: Option<SelectedGoal> = None;

    for definition in &domain.goals {
        if !goal_valid(world, entity, domain_id, definition, state, current_goal) {
            continue;
        }
        if goal_completed(world, entity, domain_id, definition, state, current_goal) {
            continue;
        }

        let score = goal_score(world, entity, domain_id, definition, state, current_goal);
        let candidate = SelectedGoal {
            id: definition.id,
            name: definition.name.clone(),
            priority: definition.priority,
            score,
        };

        match best.as_ref() {
            None => best = Some(candidate),
            Some(previous) => {
                if candidate.score > previous.score
                    || (candidate.score == previous.score && candidate.priority > previous.priority)
                {
                    best = Some(candidate);
                }
            }
        }
    }

    best
}

fn goal_score(
    world: &mut World,
    entity: Entity,
    domain_id: GoapDomainId,
    goal: &GoalDefinition,
    state: &GoapWorldState,
    current_goal: Option<&SelectedGoal>,
) -> f32 {
    let base = goal.priority as f32;
    let Some(hook_key) = &goal.relevance else {
        return base;
    };
    let Some(handler) = world.resource::<GoapHooks>().goal_score(hook_key).cloned() else {
        return base;
    };

    base + handler(
        world,
        GoalHookContext {
            entity,
            domain_id,
            state: state.clone(),
            active_goal: current_goal.cloned(),
            goal: goal.clone(),
        },
    )
}

fn goal_valid(
    world: &mut World,
    entity: Entity,
    domain_id: GoapDomainId,
    goal: &GoalDefinition,
    state: &GoapWorldState,
    current_goal: Option<&SelectedGoal>,
) -> bool {
    let Some(hook_key) = &goal.validator else {
        return true;
    };
    let Some(handler) = world
        .resource::<GoapHooks>()
        .goal_validator(hook_key)
        .cloned()
    else {
        return true;
    };

    handler(
        world,
        GoalHookContext {
            entity,
            domain_id,
            state: state.clone(),
            active_goal: current_goal.cloned(),
            goal: goal.clone(),
        },
    )
}

fn goal_completed(
    world: &mut World,
    entity: Entity,
    domain_id: GoapDomainId,
    goal: &GoalDefinition,
    state: &GoapWorldState,
    current_goal: Option<&SelectedGoal>,
) -> bool {
    if let Some(hook_key) = &goal.completion {
        if let Some(handler) = world
            .resource::<GoapHooks>()
            .goal_completion(hook_key)
            .cloned()
        {
            return handler(
                world,
                GoalHookContext {
                    entity,
                    domain_id,
                    state: state.clone(),
                    active_goal: current_goal.cloned(),
                    goal: goal.clone(),
                },
            );
        }
    }

    goal.desired_state
        .iter()
        .all(|condition| condition.matches(state))
}

fn request_plan(world: &mut World, entity: Entity) {
    world.resource_mut::<GoapPlannerScheduler>().enqueue(entity);
    if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
        runtime.status = PlannerStatus::QueuedForPlanning;
    }
}

fn set_goal(world: &mut World, entity: Entity, new_goal: Option<SelectedGoal>) {
    let previous_goal = world
        .get::<GoapRuntime>(entity)
        .and_then(|runtime| runtime.current_goal.clone());

    if previous_goal == new_goal {
        return;
    }

    if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
        runtime.current_goal = new_goal.clone();
        runtime.last_failed_goal = None;
        runtime.last_failed_revision = None;
    }

    emit_message(
        world,
        GoalChanged {
            entity,
            previous_goal,
            new_goal,
        },
    );
}

fn build_planning_problem(
    world: &mut World,
    entity: Entity,
    agent: &GoapAgent,
    domain: &GoapDomainDefinition,
) -> Option<PlanningProblem> {
    let (state, goal) = {
        let runtime = world.get::<GoapRuntime>(entity)?;
        (
            runtime_effective_state(domain, runtime),
            runtime.current_goal.clone()?,
        )
    };
    let goal_definition = domain.goals.get(goal.id.0)?.clone();

    let mut actions = Vec::new();
    for action in &domain.actions {
        if let Some(target_spec) = &action.target {
            let targets = world
                .resource::<GoapHooks>()
                .target_provider(&target_spec.provider)
                .cloned()
                .map(|handler| {
                    handler(
                        world,
                        TargetProviderContext {
                            entity,
                            domain_id: agent.domain,
                            state: state.clone(),
                            goal: goal.clone(),
                            action: action.clone(),
                        },
                    )
                })
                .unwrap_or_default();

            for (candidate_index, target) in targets.into_iter().enumerate() {
                if !action_context_valid(
                    world,
                    entity,
                    agent.domain,
                    &state,
                    &goal,
                    action,
                    Some(target.clone()),
                ) {
                    continue;
                }
                actions.push(PreparedActionVariant {
                    action_id: action.id,
                    action_name: action.name.clone(),
                    executor: action.executor.clone(),
                    preconditions: action.preconditions.clone(),
                    effects: action.effects.clone(),
                    cost: resolved_action_cost(
                        world,
                        entity,
                        agent.domain,
                        &state,
                        &goal,
                        action,
                        Some(target.clone()),
                    ),
                    target_slot: Some(target_spec.slot.clone()),
                    target: Some(target),
                    sort_index: action.id.0 * 1000 + candidate_index,
                });
            }
        } else if action_context_valid(world, entity, agent.domain, &state, &goal, action, None) {
            actions.push(PreparedActionVariant {
                action_id: action.id,
                action_name: action.name.clone(),
                executor: action.executor.clone(),
                preconditions: action.preconditions.clone(),
                effects: action.effects.clone(),
                cost: resolved_action_cost(
                    world,
                    entity,
                    agent.domain,
                    &state,
                    &goal,
                    action,
                    None,
                ),
                target_slot: None,
                target: None,
                sort_index: action.id.0 * 1000,
            });
        }
    }

    Some(PlanningProblem {
        initial_state: state,
        state_revision: world
            .get::<GoapRuntime>(entity)
            .map(|runtime| runtime.sensor_revision)
            .unwrap_or_default(),
        goal,
        desired_state: goal_definition.desired_state,
        actions,
        limits: agent
            .config
            .resolve_planner_limits(domain.default_planner_limits),
    })
}

fn action_context_valid(
    world: &mut World,
    entity: Entity,
    domain_id: GoapDomainId,
    state: &GoapWorldState,
    goal: &SelectedGoal,
    action: &ActionDefinition,
    target: Option<TargetCandidate>,
) -> bool {
    let Some(hook_key) = &action.context_validator else {
        return true;
    };
    let Some(handler) = world
        .resource::<GoapHooks>()
        .action_validator(hook_key)
        .cloned()
    else {
        return true;
    };

    handler(
        world,
        ActionEvaluationContext {
            entity,
            domain_id,
            state: state.clone(),
            goal: goal.clone(),
            action: action.clone(),
            target,
        },
    )
}

fn resolved_action_cost(
    world: &mut World,
    entity: Entity,
    domain_id: GoapDomainId,
    state: &GoapWorldState,
    goal: &SelectedGoal,
    action: &ActionDefinition,
    target: Option<TargetCandidate>,
) -> u32 {
    let mut cost = action.base_cost.max(1) as i32;
    if let Some(target) = target.as_ref() {
        cost += target.cost_bias as i32;
    }

    if let Some(hook_key) = &action.dynamic_cost {
        if let Some(handler) = world.resource::<GoapHooks>().action_cost(hook_key).cloned() {
            cost += handler(
                world,
                ActionEvaluationContext {
                    entity,
                    domain_id,
                    state: state.clone(),
                    goal: goal.clone(),
                    action: action.clone(),
                    target,
                },
            );
        }
    }

    cost.max(1) as u32
}

fn apply_successful_plan(world: &mut World, entity: Entity, draft: GoapPlanDraft) {
    let cost = draft.total_cost;
    let length = draft.steps.len();
    let goal = draft.goal.clone();
    let sensor_revision = world
        .get::<GoapRuntime>(entity)
        .map(|runtime| runtime.sensor_revision)
        .unwrap_or_default();

    if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
        runtime.status = PlannerStatus::Dispatching;
        runtime.counters.replans += 1;
        runtime.counters.last_expansions = draft.expansions;
        runtime.counters.total_expansions += u64::from(draft.expansions);
        runtime.last_failed_goal = None;
        runtime.last_failed_revision = None;
        runtime.current_plan = Some(GoapPlan::from_draft(draft, sensor_revision));
        runtime.planning_session = None;
    }

    emit_message(
        world,
        PlanStarted {
            entity,
            goal,
            cost,
            length,
        },
    );
}

fn handle_action_report(world: &mut World, report: ActionExecutionReport) {
    let Some(matches_ticket) = world
        .get::<GoapRuntime>(report.entity)
        .and_then(|runtime| runtime.active_action.as_ref())
        .map(|action| action.ticket == report.ticket)
    else {
        return;
    };

    if !matches_ticket {
        return;
    }

    let Some(mut runtime) = world.get_mut::<GoapRuntime>(report.entity) else {
        return;
    };

    let Some(active_action) = runtime.active_action.as_mut() else {
        return;
    };

    active_action.note = report.note.clone();
    match report.status {
        ActionExecutionStatus::Running => {
            active_action.status = ActiveActionStatus::Running;
            runtime.status = PlannerStatus::WaitingOnAction;
        }
        ActionExecutionStatus::Waiting => {
            active_action.status = ActiveActionStatus::Waiting;
            runtime.status = PlannerStatus::WaitingOnAction;
        }
        ActionExecutionStatus::Success => {
            runtime.active_action = None;
            if let Some(plan) = runtime.current_plan.as_mut() {
                plan.advance();
            }
            runtime.status = PlannerStatus::Dispatching;
        }
        ActionExecutionStatus::Failure { reason } => {
            let _ = runtime;
            invalidate_plan(
                world,
                report.entity,
                PlanInvalidationReason::ActionFailed { reason },
                true,
            );
        }
        ActionExecutionStatus::Cancelled { reason } => {
            invalidate_plan(
                world,
                report.entity,
                PlanInvalidationReason::Manual { reason },
                false,
            );
        }
    }
}

fn invalidate_plan(
    world: &mut World,
    entity: Entity,
    reason: PlanInvalidationReason,
    queue_replan: bool,
) {
    let Some((goal, active_action, action_id, action_name, ticket)) =
        world.get::<GoapRuntime>(entity).map(|runtime| {
            (
                runtime.current_goal.clone(),
                runtime.active_action.clone(),
                runtime
                    .active_action
                    .as_ref()
                    .map(|action| action.action_id),
                runtime
                    .active_action
                    .as_ref()
                    .map(|action| action.action_name.clone()),
                runtime.active_action.as_ref().map(|action| action.ticket),
            )
        })
    else {
        return;
    };

    if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
        runtime.current_plan = None;
        runtime.active_action = None;
        runtime.planning_session = None;
        runtime.last_failed_goal = None;
        runtime.last_failed_revision = None;
        runtime.last_invalidation_reason = Some(reason.clone());
        runtime.status = PlannerStatus::SelectingGoal;
        runtime.counters.invalidations += 1;
    }

    emit_message(
        world,
        PlanInvalidated {
            entity,
            goal: goal.clone(),
            reason: reason.clone(),
        },
    );

    if let (Some(_), Some(action_id), Some(action_name), Some(ticket)) =
        (active_action, action_id, action_name, ticket)
    {
        emit_message(
            world,
            ActionCancelled {
                entity,
                ticket,
                action_id,
                action_name,
                reason: reason.clone(),
            },
        );
    }

    if queue_replan && goal.is_some() {
        request_plan(world, entity);
    }
}

fn finish_goal(world: &mut World, entity: Entity) {
    let goal = world
        .get::<GoapRuntime>(entity)
        .and_then(|runtime| runtime.current_goal.clone());
    let Some(goal) = goal else {
        return;
    };

    if let Some(mut runtime) = world.get_mut::<GoapRuntime>(entity) {
        runtime.current_plan = None;
        runtime.active_action = None;
        runtime.planning_session = None;
        runtime.current_goal = None;
        runtime.last_failed_goal = None;
        runtime.last_failed_revision = None;
        runtime.status = PlannerStatus::Completed;
        runtime.counters.completed_plans += 1;
        runtime.last_invalidation_reason = Some(PlanInvalidationReason::GoalCompleted);
    }

    emit_message(
        world,
        PlanCompleted {
            entity,
            goal: goal.clone(),
        },
    );
    emit_message(
        world,
        GoalChanged {
            entity,
            previous_goal: Some(goal.clone()),
            new_goal: None,
        },
    );
}

fn step_target_is_still_valid(
    world: &mut World,
    entity: Entity,
    domain_id: GoapDomainId,
    domain: &GoapDomainDefinition,
    goal: &SelectedGoal,
    step: &crate::planner::GoapPlanStep,
) -> bool {
    let Some(target) = step.target.clone() else {
        return true;
    };
    let Some(action) = domain.actions.get(step.action_id.0).cloned() else {
        return false;
    };
    let Some(target_spec) = action.target.clone() else {
        return false;
    };

    let Some(state) = world
        .get::<GoapRuntime>(entity)
        .map(|runtime| runtime_effective_state(domain, runtime))
    else {
        return false;
    };

    let provider_valid = world
        .resource::<GoapHooks>()
        .target_provider(&target_spec.provider)
        .cloned()
        .map(|handler| {
            handler(
                world,
                TargetProviderContext {
                    entity,
                    domain_id,
                    state: state.clone(),
                    goal: goal.clone(),
                    action: action.clone(),
                },
            )
            .into_iter()
            .any(|candidate| candidate.token == target.token)
        })
        .unwrap_or(false);
    if !provider_valid {
        return false;
    }

    action_context_valid(
        world,
        entity,
        domain_id,
        &state,
        goal,
        &action,
        Some(target),
    )
}

fn first_failed_precondition(
    domain: &GoapDomainDefinition,
    plan: &GoapPlan,
    state: &GoapWorldState,
) -> Option<PlanInvalidationReason> {
    let step = plan.current_step()?;
    let action = domain.actions.get(step.action_id.0)?;
    action
        .preconditions
        .iter()
        .find(|condition| !condition.matches(state))
        .map(|condition| PlanInvalidationReason::RequiredFactChanged { key: condition.key })
}

fn collect_action_reports(world: &mut World) -> Vec<ActionExecutionReport> {
    let mut cursor = {
        let mut cursors = world.resource_mut::<GoapMessageCursors>();
        std::mem::take(&mut cursors.action_reports)
    };
    let reports = {
        let messages = world.resource::<Messages<ActionExecutionReport>>();
        cursor.read(messages).cloned().collect::<Vec<_>>()
    };
    world.resource_mut::<GoapMessageCursors>().action_reports = cursor;
    reports
}

fn collect_agent_invalidations(world: &mut World) -> Vec<InvalidateGoapAgent> {
    let mut cursor = {
        let mut cursors = world.resource_mut::<GoapMessageCursors>();
        std::mem::take(&mut cursors.invalidate_agents)
    };
    let invalidations = {
        let messages = world.resource::<Messages<InvalidateGoapAgent>>();
        cursor.read(messages).cloned().collect::<Vec<_>>()
    };
    world.resource_mut::<GoapMessageCursors>().invalidate_agents = cursor;
    invalidations
}

fn collect_local_invalidations(world: &mut World) -> HashSet<Entity> {
    let mut cursor = {
        let mut cursors = world.resource_mut::<GoapMessageCursors>();
        std::mem::take(&mut cursors.invalidate_local_sensors)
    };
    let invalidations = {
        let messages = world.resource::<Messages<InvalidateLocalSensors>>();
        cursor
            .read(messages)
            .map(|message| message.entity)
            .collect()
    };
    world
        .resource_mut::<GoapMessageCursors>()
        .invalidate_local_sensors = cursor;
    invalidations
}

fn collect_global_invalidations(world: &mut World) -> HashSet<GoapDomainId> {
    let mut cursor = {
        let mut cursors = world.resource_mut::<GoapMessageCursors>();
        std::mem::take(&mut cursors.invalidate_global_sensors)
    };
    let invalidations = {
        let messages = world.resource::<Messages<InvalidateGlobalSensors>>();
        cursor
            .read(messages)
            .map(|message| message.domain)
            .collect()
    };
    world
        .resource_mut::<GoapMessageCursors>()
        .invalidate_global_sensors = cursor;
    invalidations
}

fn apply_sensor_output(state: &mut GoapWorldState, output: &SensorOutput) -> bool {
    let before = state.clone();
    for patch in &output.patches {
        patch.apply(state);
    }
    *state != before
}

fn emit_message<T: Message>(world: &mut World, message: T) {
    world.resource_mut::<Messages<T>>().write(message);
}

fn clone_domain(world: &World, domain_id: GoapDomainId) -> Option<GoapDomainDefinition> {
    world.resource::<GoapLibrary>().domain(domain_id).cloned()
}

fn agent_entities(world: &mut World) -> Vec<Entity> {
    world
        .query_filtered::<Entity, With<GoapAgent>>()
        .iter(world)
        .collect::<Vec<_>>()
}

type GoalSelectionSnapshot = (
    GoapWorldState,
    Option<SelectedGoal>,
    bool,
    Option<ActiveAction>,
    PlannerStatus,
    Option<crate::definitions::GoalId>,
    Option<u64>,
    Option<u64>,
    Option<u64>,
    u64,
);

fn runtime_snapshot(
    world: &World,
    domain: &GoapDomainDefinition,
    entity: Entity,
) -> Option<GoalSelectionSnapshot> {
    let runtime = world.get::<GoapRuntime>(entity)?;
    Some((
        runtime_effective_state(domain, runtime),
        runtime.current_goal.clone(),
        runtime.current_plan.is_some(),
        runtime.active_action.clone(),
        runtime.status.clone(),
        runtime.last_failed_goal,
        runtime.last_failed_revision,
        runtime
            .current_plan
            .as_ref()
            .map(|plan| plan.built_from_revision),
        runtime
            .planning_session
            .as_ref()
            .map(PlanningSession::source_revision),
        runtime.sensor_revision,
    ))
}

fn runtime_monitor_snapshot(
    world: &World,
    domain: &GoapDomainDefinition,
    entity: Entity,
) -> Option<(
    GoapWorldState,
    Option<SelectedGoal>,
    Option<GoapPlan>,
    Option<ActiveAction>,
)> {
    let runtime = world.get::<GoapRuntime>(entity)?;
    Some((
        runtime_effective_state(domain, runtime),
        runtime.current_goal.clone(),
        runtime.current_plan.clone(),
        runtime.active_action.clone(),
    ))
}

fn runtime_debug_snapshot(
    world: &World,
    domain: &GoapDomainDefinition,
    entity: Entity,
) -> Option<(
    PlannerStatus,
    Option<SelectedGoal>,
    Option<GoapPlan>,
    Option<PlanInvalidationReason>,
    crate::components::GoapCounters,
    GoapWorldState,
    Option<ActiveAction>,
)> {
    let runtime = world.get::<GoapRuntime>(entity)?;
    Some((
        runtime.status.clone(),
        runtime.current_goal.clone(),
        runtime.current_plan.clone(),
        runtime.last_invalidation_reason.clone(),
        runtime.counters.clone(),
        runtime_effective_state(domain, runtime),
        runtime.active_action.clone(),
    ))
}

fn runtime_effective_state(domain: &GoapDomainDefinition, runtime: &GoapRuntime) -> GoapWorldState {
    let mut state = runtime.sensed_state.clone();
    let Some(plan) = runtime.current_plan.as_ref() else {
        return state;
    };

    for step in plan.steps.iter().take(plan.cursor) {
        let Some(action) = domain.actions.get(step.action_id.0) else {
            continue;
        };
        for effect in &action.effects {
            effect.apply(&mut state);
        }
    }

    state
}

fn time_seconds(world: &World) -> f32 {
    world
        .get_resource::<Time>()
        .map(Time::elapsed_secs_wrapped)
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "systems_tests.rs"]
mod tests;
