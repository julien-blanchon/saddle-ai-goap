use bevy::prelude::*;
use saddle_bevy_e2e::action::Action;

use crate::{GoapLabDiagnostics, LabOverlay};

pub(super) fn overlay_text(world: &mut World) -> Option<String> {
    let mut query = world.query_filtered::<&Text, With<LabOverlay>>();
    query.iter(world).next().map(|text| text.0.clone())
}

pub(super) fn wait_for_core_plans() -> Action {
    Action::WaitUntil {
        label: "guard and worker plans ready".into(),
        condition: Box::new(|world| {
            let diagnostics = world.resource::<GoapLabDiagnostics>();
            diagnostics.guard_plan_starts > 0 && diagnostics.worker_plan_starts > 0
        }),
        max_frames: 90,
    }
}

pub(super) fn wait_for_guard_plan_started() -> Action {
    Action::WaitUntil {
        label: "guard plan started".into(),
        condition: Box::new(|world| world.resource::<GoapLabDiagnostics>().guard_plan_starts > 0),
        max_frames: 90,
    }
}

pub(super) fn wait_for_guard_invalidation() -> Action {
    Action::WaitUntil {
        label: "guard invalidated with target reason".into(),
        condition: Box::new(|world| {
            let diagnostics = world.resource::<GoapLabDiagnostics>();
            diagnostics.guard_plan_invalidations >= 1
                && diagnostics
                    .guard_last_invalidation
                    .as_deref()
                    .is_some_and(|reason| reason.contains("TargetInvalidated"))
        }),
        max_frames: 180,
    }
}

pub(super) fn wait_for_guard_completion() -> Action {
    Action::WaitUntil {
        label: "guard completed fallback route".into(),
        condition: Box::new(|world| {
            let diagnostics = world.resource::<GoapLabDiagnostics>();
            diagnostics.guard_plan_starts >= 2
                && diagnostics.guard_plan_completions >= 1
                && diagnostics.guard_targets_remaining == 0
        }),
        max_frames: 240,
    }
}

pub(super) fn wait_for_worker_blocked() -> Action {
    Action::WaitUntil {
        label: "worker blocked by workbench".into(),
        condition: Box::new(|world| {
            let diagnostics = world.resource::<GoapLabDiagnostics>();
            diagnostics.worker_plan_invalidations >= 1 && !diagnostics.workbench_available
        }),
        max_frames: 180,
    }
}

pub(super) fn wait_for_worker_delivery() -> Action {
    Action::WaitUntil {
        label: "worker delivered and recovered".into(),
        condition: Box::new(|world| {
            let diagnostics = world.resource::<GoapLabDiagnostics>();
            diagnostics.worker_plan_completions >= 1
                && diagnostics.worker_deposited
                && diagnostics.workbench_available
        }),
        max_frames: 300,
    }
}
