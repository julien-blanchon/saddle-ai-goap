use super::*;
use crate::definitions::{ActionId, GoalId, HookKey};
use crate::world_state::{FactCondition, FactEffect, GoapWorldState, WorldKeyId};

fn make_state(pairs: &[(WorldKeyId, bool)]) -> GoapWorldState {
    let mut state = GoapWorldState::with_capacity(4);
    for (key, value) in pairs {
        state.set_bool(*key, *value);
    }
    state
}

fn make_goal(name: &str) -> SelectedGoal {
    SelectedGoal {
        id: GoalId(0),
        name: name.into(),
        priority: 10,
        score: 10.0,
    }
}

fn action(
    id: usize,
    name: &str,
    cost: u32,
    preconditions: Vec<FactCondition>,
    effects: Vec<FactEffect>,
    sort_index: usize,
) -> PreparedActionVariant {
    PreparedActionVariant {
        action_id: ActionId(id),
        action_name: name.into(),
        executor: HookKey::new(name),
        preconditions,
        effects,
        cost,
        target_slot: None,
        target: None,
        sort_index,
        interruptible: true,
    }
}

fn run_to_completion(mut session: PlanningSession) -> PlanningStepOutcome {
    loop {
        match session.step(64) {
            PlanningStepOutcome::InProgress { .. } => continue,
            outcome => return outcome,
        }
    }
}

#[test]
fn linear_chain_plan_is_found() {
    let has_tool = WorldKeyId(0);
    let built_item = WorldKeyId(1);
    let problem = PlanningProblem {
        initial_state: make_state(&[]),
        state_revision: 0,
        goal: make_goal("build_item"),
        desired_state: vec![FactCondition::equals_bool(built_item, true)],
        actions: vec![
            action(
                0,
                "pick_up_tool",
                1,
                vec![],
                vec![FactEffect::set_bool(has_tool, true)],
                0,
            ),
            action(
                1,
                "build_item",
                1,
                vec![FactCondition::equals_bool(has_tool, true)],
                vec![FactEffect::set_bool(built_item, true)],
                1,
            ),
        ],
        limits: GoapPlannerLimits::default(),
    };

    let outcome = run_to_completion(PlanningSession::new(problem));
    let PlanningStepOutcome::Success(plan) = outcome else {
        panic!("expected a successful plan");
    };
    assert_eq!(plan.total_cost, 2);
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(&*plan.steps[0].action_name, "pick_up_tool");
    assert_eq!(&*plan.steps[1].action_name, "build_item");
}

#[test]
fn cheaper_plan_is_selected() {
    let goal_key = WorldKeyId(0);
    let helper_key = WorldKeyId(1);
    let problem = PlanningProblem {
        initial_state: make_state(&[]),
        state_revision: 0,
        goal: make_goal("cheap"),
        desired_state: vec![FactCondition::equals_bool(goal_key, true)],
        actions: vec![
            action(
                0,
                "expensive_direct",
                5,
                vec![],
                vec![FactEffect::set_bool(goal_key, true)],
                0,
            ),
            action(
                1,
                "cheap_setup",
                1,
                vec![],
                vec![FactEffect::set_bool(helper_key, true)],
                1,
            ),
            action(
                2,
                "cheap_finish",
                1,
                vec![FactCondition::equals_bool(helper_key, true)],
                vec![FactEffect::set_bool(goal_key, true)],
                2,
            ),
        ],
        limits: GoapPlannerLimits::default(),
    };

    let outcome = run_to_completion(PlanningSession::new(problem));
    let PlanningStepOutcome::Success(plan) = outcome else {
        panic!("expected a successful plan");
    };
    assert_eq!(plan.total_cost, 2);
    assert_eq!(
        plan.steps
            .iter()
            .map(|step| &*step.action_name)
            .collect::<Vec<_>>(),
        vec!["cheap_setup", "cheap_finish"]
    );
}

#[test]
fn no_plan_is_reported_when_preconditions_cannot_be_met() {
    let goal_key = WorldKeyId(0);
    let missing_key = WorldKeyId(1);
    let problem = PlanningProblem {
        initial_state: make_state(&[]),
        state_revision: 0,
        goal: make_goal("unreachable"),
        desired_state: vec![FactCondition::equals_bool(goal_key, true)],
        actions: vec![action(
            0,
            "blocked",
            1,
            vec![FactCondition::equals_bool(missing_key, true)],
            vec![FactEffect::set_bool(goal_key, true)],
            0,
        )],
        limits: GoapPlannerLimits::default(),
    };

    let outcome = run_to_completion(PlanningSession::new(problem));
    let PlanningStepOutcome::Failure { reason, .. } = outcome else {
        panic!("expected failure");
    };
    assert_eq!(reason, PlanningFailureReason::NoPlan);
}

#[test]
fn deterministic_tie_breaking_uses_action_order() {
    let goal_key = WorldKeyId(0);
    let problem = PlanningProblem {
        initial_state: make_state(&[]),
        state_revision: 0,
        goal: make_goal("tie"),
        desired_state: vec![FactCondition::equals_bool(goal_key, true)],
        actions: vec![
            action(
                0,
                "first",
                1,
                vec![],
                vec![FactEffect::set_bool(goal_key, true)],
                0,
            ),
            action(
                1,
                "second",
                1,
                vec![],
                vec![FactEffect::set_bool(goal_key, true)],
                1,
            ),
        ],
        limits: GoapPlannerLimits::default(),
    };

    let outcome = run_to_completion(PlanningSession::new(problem));
    let PlanningStepOutcome::Success(plan) = outcome else {
        panic!("expected success");
    };
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(&*plan.steps[0].action_name, "first");
}

#[test]
fn node_expansion_budget_is_enforced() {
    let goal_key = WorldKeyId(0);
    let loop_key = WorldKeyId(1);
    let problem = PlanningProblem {
        initial_state: make_state(&[]),
        state_revision: 0,
        goal: make_goal("budget"),
        desired_state: vec![FactCondition::equals_bool(goal_key, true)],
        actions: vec![action(
            0,
            "toggle",
            1,
            vec![],
            vec![FactEffect::set_bool(loop_key, true)],
            0,
        )],
        limits: GoapPlannerLimits {
            max_node_expansions: 0,
            max_plan_length: 4,
            max_expansions_per_step: 1,
        },
    };

    let outcome = PlanningSession::new(problem).step(1);
    let PlanningStepOutcome::Failure { reason, .. } = outcome else {
        panic!("expected failure");
    };
    assert_eq!(reason, PlanningFailureReason::MaxNodeExpansions);
}

#[test]
fn plan_length_guardrail_is_reported() {
    let a = WorldKeyId(0);
    let b = WorldKeyId(1);
    let goal_key = WorldKeyId(2);
    let problem = PlanningProblem {
        initial_state: make_state(&[]),
        state_revision: 0,
        goal: make_goal("length"),
        desired_state: vec![FactCondition::equals_bool(goal_key, true)],
        actions: vec![
            action(0, "a", 1, vec![], vec![FactEffect::set_bool(a, true)], 0),
            action(
                1,
                "b",
                1,
                vec![FactCondition::equals_bool(a, true)],
                vec![FactEffect::set_bool(b, true)],
                1,
            ),
            action(
                2,
                "goal",
                1,
                vec![FactCondition::equals_bool(b, true)],
                vec![FactEffect::set_bool(goal_key, true)],
                2,
            ),
        ],
        limits: GoapPlannerLimits {
            max_node_expansions: 32,
            max_plan_length: 1,
            max_expansions_per_step: 16,
        },
    };

    let outcome = run_to_completion(PlanningSession::new(problem));
    let PlanningStepOutcome::Failure { reason, .. } = outcome else {
        panic!("expected failure");
    };
    assert_eq!(reason, PlanningFailureReason::MaxPlanLength);
}

#[test]
fn planning_session_reports_in_progress_before_success() {
    let has_tool = WorldKeyId(0);
    let built_item = WorldKeyId(1);
    let problem = PlanningProblem {
        initial_state: make_state(&[]),
        state_revision: 7,
        goal: make_goal("build_item"),
        desired_state: vec![FactCondition::equals_bool(built_item, true)],
        actions: vec![
            action(
                0,
                "pick_up_tool",
                1,
                vec![],
                vec![FactEffect::set_bool(has_tool, true)],
                0,
            ),
            action(
                1,
                "build_item",
                1,
                vec![FactCondition::equals_bool(has_tool, true)],
                vec![FactEffect::set_bool(built_item, true)],
                1,
            ),
        ],
        limits: GoapPlannerLimits {
            max_node_expansions: 32,
            max_plan_length: 8,
            max_expansions_per_step: 1,
        },
    };

    let mut session = PlanningSession::new(problem);
    assert_eq!(session.source_revision(), 7);

    let first = session.step(1);
    assert!(matches!(first, PlanningStepOutcome::InProgress { .. }));

    let second = session.step(1);
    assert!(matches!(
        second,
        PlanningStepOutcome::InProgress { .. } | PlanningStepOutcome::Success(_)
    ));

    let final_outcome = run_to_completion(session);
    assert!(matches!(final_outcome, PlanningStepOutcome::Success(_)));
}

#[test]
fn heuristic_uses_action_costs_not_just_count() {
    // With the old count-based heuristic, both conditions contribute h=1 each.
    // With h_max, the heuristic accounts for actual action costs.
    let a = WorldKeyId(0);
    let b = WorldKeyId(1);
    let problem = PlanningProblem {
        initial_state: make_state(&[]),
        state_revision: 0,
        goal: make_goal("two_goals"),
        desired_state: vec![
            FactCondition::equals_bool(a, true),
            FactCondition::equals_bool(b, true),
        ],
        actions: vec![
            action(
                0,
                "expensive_a",
                10,
                vec![],
                vec![FactEffect::set_bool(a, true)],
                0,
            ),
            action(
                1,
                "cheap_b",
                1,
                vec![],
                vec![FactEffect::set_bool(b, true)],
                1,
            ),
        ],
        limits: GoapPlannerLimits::default(),
    };

    let session = PlanningSession::new(problem);
    // h_max should report 10 (the max of min-costs [10, 1]) for the initial state
    let h = heuristic(
        &session.problem().initial_state,
        &session.problem().desired_state,
        &session.relevance_map,
    );
    assert_eq!(h, 10);

    let outcome = run_to_completion(session);
    let PlanningStepOutcome::Success(plan) = outcome else {
        panic!("expected success");
    };
    assert_eq!(plan.total_cost, 11);
}

#[test]
fn relevance_map_reports_max_for_unsatisfiable_condition() {
    let goal_key = WorldKeyId(0);
    let unrelated = WorldKeyId(1);

    // The only action sets `unrelated`, not `goal_key`. No action can satisfy the goal.
    let map = ActionRelevanceMap::build(
        &[FactCondition::equals_bool(goal_key, true)],
        &[action(
            0,
            "wrong",
            1,
            vec![],
            vec![FactEffect::set_bool(unrelated, true)],
            0,
        )],
    );
    assert_eq!(map.min_costs[0], u32::MAX);
}

#[test]
fn effect_can_satisfy_covers_all_comparisons() {
    use crate::world_state::{FactComparison, TargetToken};

    let key = WorldKeyId(0);

    // Set(bool) satisfies Equals(bool)
    assert!(effect_can_satisfy(
        &FactEffect::set_bool(key, true),
        &FactCondition::equals_bool(key, true),
    ));
    assert!(!effect_can_satisfy(
        &FactEffect::set_bool(key, false),
        &FactCondition::equals_bool(key, true),
    ));

    // Set(int) satisfies GreaterOrEqual
    assert!(effect_can_satisfy(
        &FactEffect::set_int(key, 10),
        &FactCondition::int_at_least(key, 5),
    ));
    assert!(!effect_can_satisfy(
        &FactEffect::set_int(key, 3),
        &FactCondition::int_at_least(key, 5),
    ));

    // AddInt satisfies GreaterOrEqual (positive delta)
    assert!(effect_can_satisfy(
        &FactEffect::add_int(key, 1),
        &FactCondition::int_at_least(key, 5),
    ));
    assert!(!effect_can_satisfy(
        &FactEffect::add_int(key, -1),
        &FactCondition::int_at_least(key, 5),
    ));

    // Clear satisfies IsUnset
    assert!(effect_can_satisfy(
        &FactEffect::clear(key),
        &FactCondition::is_unset(key),
    ));

    // Set satisfies IsSet
    assert!(effect_can_satisfy(
        &FactEffect::set_int(key, 1),
        &FactCondition::is_set(key),
    ));

    // Wrong key never satisfies
    let other = WorldKeyId(1);
    assert!(!effect_can_satisfy(
        &FactEffect::set_bool(other, true),
        &FactCondition::equals_bool(key, true),
    ));
}
