use std::sync::Arc;

use bevy::prelude::*;

use crate::definitions::{ActionDefinition, GoalDefinition, GoapDomainId};
use crate::planner::{SelectedGoal, TargetCandidate};
use crate::world_state::{FactPatch, GoapWorldState};

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct SensorOutput {
    pub patches: Vec<FactPatch>,
    pub note: Option<String>,
}

impl SensorOutput {
    pub fn new(patches: impl IntoIterator<Item = FactPatch>) -> Self {
        Self {
            patches: patches.into_iter().collect(),
            note: None,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct LocalSensorContext {
    pub entity: Entity,
    pub domain_id: GoapDomainId,
    pub current_state: GoapWorldState,
    pub global_state: GoapWorldState,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct GlobalSensorContext {
    pub domain_id: GoapDomainId,
    pub current_state: GoapWorldState,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct GoalHookContext {
    pub entity: Entity,
    pub domain_id: GoapDomainId,
    pub state: GoapWorldState,
    pub active_goal: Option<SelectedGoal>,
    pub goal: GoalDefinition,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct ActionEvaluationContext {
    pub entity: Entity,
    pub domain_id: GoapDomainId,
    pub state: GoapWorldState,
    pub goal: SelectedGoal,
    pub action: ActionDefinition,
    pub target: Option<TargetCandidate>,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct TargetProviderContext {
    pub entity: Entity,
    pub domain_id: GoapDomainId,
    pub state: GoapWorldState,
    pub goal: SelectedGoal,
    pub action: ActionDefinition,
}

pub type LocalSensorHandler =
    Arc<dyn Fn(&mut World, LocalSensorContext) -> SensorOutput + Send + Sync>;
pub type GlobalSensorHandler =
    Arc<dyn Fn(&mut World, GlobalSensorContext) -> SensorOutput + Send + Sync>;
pub type GoalScoreHandler = Arc<dyn Fn(&mut World, GoalHookContext) -> f32 + Send + Sync>;
pub type GoalPredicateHandler = Arc<dyn Fn(&mut World, GoalHookContext) -> bool + Send + Sync>;
pub type TargetProviderHandler =
    Arc<dyn Fn(&mut World, TargetProviderContext) -> Vec<TargetCandidate> + Send + Sync>;
pub type ActionPredicateHandler =
    Arc<dyn Fn(&mut World, ActionEvaluationContext) -> bool + Send + Sync>;
pub type ActionCostHandler = Arc<dyn Fn(&mut World, ActionEvaluationContext) -> i32 + Send + Sync>;
