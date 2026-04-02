use bevy::prelude::*;

use crate::components::{PlanInvalidationReason, PlannerStatus};
use crate::definitions::{ActionId, GoalId, GoapDomainId, HookKey};
use crate::planner::{SelectedGoal, TargetCandidate};

#[derive(Debug, Clone, PartialEq, Eq, Reflect)]
pub enum ActionExecutionStatus {
    Running,
    Waiting,
    Success,
    Failure { reason: String },
    Cancelled { reason: String },
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct ActionExecutionReport {
    pub entity: Entity,
    pub ticket: u64,
    pub status: ActionExecutionStatus,
    pub note: Option<String>,
}

impl ActionExecutionReport {
    pub fn new(entity: Entity, ticket: u64, status: ActionExecutionStatus) -> Self {
        Self {
            entity,
            ticket,
            status,
            note: None,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct InvalidateGoapAgent {
    pub entity: Entity,
    pub reason: PlanInvalidationReason,
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct InvalidateLocalSensors {
    pub entity: Entity,
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct InvalidateGlobalSensors {
    pub domain: GoapDomainId,
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct GoalChanged {
    pub entity: Entity,
    pub previous_goal: Option<SelectedGoal>,
    pub new_goal: Option<SelectedGoal>,
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct PlanStarted {
    pub entity: Entity,
    pub goal: SelectedGoal,
    pub cost: u32,
    pub length: usize,
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct PlanCompleted {
    pub entity: Entity,
    pub goal: SelectedGoal,
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct PlanFailed {
    pub entity: Entity,
    pub goal: Option<SelectedGoal>,
    pub status: PlannerStatus,
    pub reason: String,
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct PlanInvalidated {
    pub entity: Entity,
    pub goal: Option<SelectedGoal>,
    pub reason: PlanInvalidationReason,
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct ActionDispatched {
    pub entity: Entity,
    pub goal_id: GoalId,
    pub action_id: ActionId,
    pub action_name: String,
    pub executor: HookKey,
    pub ticket: u64,
    pub target_slot: Option<String>,
    pub target: Option<TargetCandidate>,
}

#[derive(Message, Debug, Clone, PartialEq, Reflect)]
pub struct ActionCancelled {
    pub entity: Entity,
    pub ticket: u64,
    pub action_id: ActionId,
    pub action_name: String,
    pub reason: PlanInvalidationReason,
}
