mod support;

use saddle_bevy_e2e::{action::Action, actions::assertions, scenario::Scenario};

use crate::GoapLabDiagnostics;

pub fn list_scenarios() -> Vec<&'static str> {
    vec![
        "smoke_launch",
        "goap_smoke",
        "goap_replan",
        "goap_worker_cycle",
    ]
}

pub fn scenario_by_name(name: &str) -> Option<Scenario> {
    match name {
        "smoke_launch" => Some(build_smoke("smoke_launch")),
        "goap_smoke" => Some(build_smoke("goap_smoke")),
        "goap_replan" => Some(goap_replan()),
        "goap_worker_cycle" => Some(goap_worker_cycle()),
        _ => None,
    }
}

fn build_smoke(name: &'static str) -> Scenario {
    Scenario::builder(name)
        .description("Boot the crate-local GOAP lab, wait for both agents to acquire plans, and capture the default planner overlay.")
        .then(support::wait_for_core_plans())
        .then(Action::Custom(Box::new(|world| {
            let diagnostics = world.resource::<GoapLabDiagnostics>();
            assert!(diagnostics.guard_plan_starts > 0);
            assert!(diagnostics.worker_plan_starts > 0);
            let overlay = support::overlay_text(world).expect("overlay text should exist");
            assert!(overlay.contains("goap lab"));
            assert!(overlay.contains("guard"));
            assert!(overlay.contains("worker"));
        })))
        .then(Action::Screenshot("smoke".into()))
        .then(Action::WaitFrames(1))
        .then(assertions::log_summary(name))
        .build()
}

fn goap_replan() -> Scenario {
    Scenario::builder("goap_replan")
        .description("Verify the guard loses its first target, invalidates the plan with a target-specific reason, replans, and completes against the fallback target.")
        .then(support::wait_for_guard_plan_started())
        .then(Action::Screenshot("guard_initial".into()))
        .then(Action::WaitFrames(1))
        .then(support::wait_for_guard_invalidation())
        .then(Action::Custom(Box::new(|world| {
            let diagnostics = world.resource::<GoapLabDiagnostics>();
            assert!(diagnostics.guard_plan_invalidations >= 1);
            assert!(
                diagnostics
                    .guard_last_invalidation
                    .as_deref()
                    .is_some_and(|reason| reason.contains("TargetInvalidated"))
            );
        })))
        .then(Action::Screenshot("guard_replan".into()))
        .then(Action::WaitFrames(1))
        .then(support::wait_for_guard_completion())
        .then(Action::Custom(Box::new(|world| {
            let diagnostics = world.resource::<GoapLabDiagnostics>();
            assert!(diagnostics.guard_plan_starts >= 2);
            assert!(diagnostics.guard_plan_completions >= 1);
            assert_eq!(diagnostics.guard_targets_remaining, 0);
        })))
        .then(Action::Screenshot("guard_resolved".into()))
        .then(Action::WaitFrames(1))
        .then(assertions::log_summary("goap_replan"))
        .build()
}

fn goap_worker_cycle() -> Scenario {
    Scenario::builder("goap_worker_cycle")
        .description("Verify the worker loses workstation availability mid-plan, invalidates, replans after the workbench returns, and eventually deposits the ingot.")
        .then(support::wait_for_worker_blocked())
        .then(Action::Custom(Box::new(|world| {
            let diagnostics = world.resource::<GoapLabDiagnostics>();
            assert!(diagnostics.worker_plan_invalidations >= 1);
            assert!(!diagnostics.workbench_available);
        })))
        .then(Action::Screenshot("worker_blocked".into()))
        .then(Action::WaitFrames(1))
        .then(support::wait_for_worker_delivery())
        .then(Action::Custom(Box::new(|world| {
            let diagnostics = world.resource::<GoapLabDiagnostics>();
            assert!(diagnostics.worker_plan_starts >= 2);
            assert!(diagnostics.worker_plan_invalidations >= 1);
            assert!(diagnostics.worker_plan_completions >= 1);
            assert!(diagnostics.worker_deposited);
        })))
        .then(Action::Screenshot("worker_complete".into()))
        .then(Action::WaitFrames(1))
        .then(assertions::log_summary("goap_worker_cycle"))
        .build()
}
