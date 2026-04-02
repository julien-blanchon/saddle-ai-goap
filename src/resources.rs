use std::collections::{HashMap, HashSet, VecDeque};

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;

use crate::components::SensorRuntimeInfo;
use crate::definitions::{GoapDomainDefinition, GoapDomainId, HookKey};
use crate::execution::{
    ActionCostHandler, ActionPredicateHandler, GlobalSensorHandler, GoalPredicateHandler,
    GoalScoreHandler, LocalSensorHandler, TargetProviderHandler,
};
use crate::messages::{
    ActionExecutionReport, InvalidateGlobalSensors, InvalidateGoapAgent, InvalidateLocalSensors,
};
use crate::world_state::GoapWorldState;

#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct GoapLibrary {
    pub domains: Vec<GoapDomainDefinition>,
}

impl GoapLibrary {
    pub fn register(&mut self, domain: GoapDomainDefinition) -> GoapDomainId {
        let id = GoapDomainId(self.domains.len());
        self.domains.push(domain);
        id
    }

    pub fn domain(&self, id: GoapDomainId) -> Option<&GoapDomainDefinition> {
        self.domains.get(id.0)
    }
}

#[derive(Resource, Default)]
pub struct GoapHooks {
    local_sensors: HashMap<String, LocalSensorHandler>,
    global_sensors: HashMap<String, GlobalSensorHandler>,
    goal_scores: HashMap<String, GoalScoreHandler>,
    goal_validators: HashMap<String, GoalPredicateHandler>,
    goal_completions: HashMap<String, GoalPredicateHandler>,
    target_providers: HashMap<String, TargetProviderHandler>,
    action_validators: HashMap<String, ActionPredicateHandler>,
    action_costs: HashMap<String, ActionCostHandler>,
}

impl GoapHooks {
    pub fn register_local_sensor<F>(&mut self, key: impl Into<HookKey>, handler: F)
    where
        F: Fn(&mut World, crate::execution::LocalSensorContext) -> crate::execution::SensorOutput
            + Send
            + Sync
            + 'static,
    {
        self.local_sensors
            .insert(key.into().0, std::sync::Arc::new(handler));
    }

    pub fn register_global_sensor<F>(&mut self, key: impl Into<HookKey>, handler: F)
    where
        F: Fn(&mut World, crate::execution::GlobalSensorContext) -> crate::execution::SensorOutput
            + Send
            + Sync
            + 'static,
    {
        self.global_sensors
            .insert(key.into().0, std::sync::Arc::new(handler));
    }

    pub fn register_goal_score<F>(&mut self, key: impl Into<HookKey>, handler: F)
    where
        F: Fn(&mut World, crate::execution::GoalHookContext) -> f32 + Send + Sync + 'static,
    {
        self.goal_scores
            .insert(key.into().0, std::sync::Arc::new(handler));
    }

    pub fn register_goal_validator<F>(&mut self, key: impl Into<HookKey>, handler: F)
    where
        F: Fn(&mut World, crate::execution::GoalHookContext) -> bool + Send + Sync + 'static,
    {
        self.goal_validators
            .insert(key.into().0, std::sync::Arc::new(handler));
    }

    pub fn register_goal_completion<F>(&mut self, key: impl Into<HookKey>, handler: F)
    where
        F: Fn(&mut World, crate::execution::GoalHookContext) -> bool + Send + Sync + 'static,
    {
        self.goal_completions
            .insert(key.into().0, std::sync::Arc::new(handler));
    }

    pub fn register_target_provider<F>(&mut self, key: impl Into<HookKey>, handler: F)
    where
        F: Fn(
                &mut World,
                crate::execution::TargetProviderContext,
            ) -> Vec<crate::planner::TargetCandidate>
            + Send
            + Sync
            + 'static,
    {
        self.target_providers
            .insert(key.into().0, std::sync::Arc::new(handler));
    }

    pub fn register_action_validator<F>(&mut self, key: impl Into<HookKey>, handler: F)
    where
        F: Fn(&mut World, crate::execution::ActionEvaluationContext) -> bool
            + Send
            + Sync
            + 'static,
    {
        self.action_validators
            .insert(key.into().0, std::sync::Arc::new(handler));
    }

    pub fn register_action_cost<F>(&mut self, key: impl Into<HookKey>, handler: F)
    where
        F: Fn(&mut World, crate::execution::ActionEvaluationContext) -> i32 + Send + Sync + 'static,
    {
        self.action_costs
            .insert(key.into().0, std::sync::Arc::new(handler));
    }

    pub fn local_sensor(&self, key: &HookKey) -> Option<&LocalSensorHandler> {
        self.local_sensors.get(key.as_str())
    }

    pub fn global_sensor(&self, key: &HookKey) -> Option<&GlobalSensorHandler> {
        self.global_sensors.get(key.as_str())
    }

    pub fn goal_score(&self, key: &HookKey) -> Option<&GoalScoreHandler> {
        self.goal_scores.get(key.as_str())
    }

    pub fn goal_validator(&self, key: &HookKey) -> Option<&GoalPredicateHandler> {
        self.goal_validators.get(key.as_str())
    }

    pub fn goal_completion(&self, key: &HookKey) -> Option<&GoalPredicateHandler> {
        self.goal_completions.get(key.as_str())
    }

    pub fn target_provider(&self, key: &HookKey) -> Option<&TargetProviderHandler> {
        self.target_providers.get(key.as_str())
    }

    pub fn action_validator(&self, key: &HookKey) -> Option<&ActionPredicateHandler> {
        self.action_validators.get(key.as_str())
    }

    pub fn action_cost(&self, key: &HookKey) -> Option<&ActionCostHandler> {
        self.action_costs.get(key.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct DomainGlobalCache {
    pub state: GoapWorldState,
    pub revision: u64,
    pub sensors: Vec<SensorRuntimeInfo>,
}

#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct GoapGlobalSensorCache {
    domains: HashMap<GoapDomainId, DomainGlobalCache>,
}

impl GoapGlobalSensorCache {
    pub fn get(&self, id: GoapDomainId) -> Option<&DomainGlobalCache> {
        self.domains.get(&id)
    }

    pub fn get_mut(&mut self, id: GoapDomainId) -> Option<&mut DomainGlobalCache> {
        self.domains.get_mut(&id)
    }

    pub fn ensure_domain(
        &mut self,
        id: GoapDomainId,
        default_state: GoapWorldState,
        sensors: Vec<SensorRuntimeInfo>,
    ) -> &mut DomainGlobalCache {
        self.domains.entry(id).or_insert(DomainGlobalCache {
            state: default_state,
            revision: 0,
            sensors,
        })
    }
}

#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct GoapPlannerScheduler {
    pub max_agents_per_frame: usize,
    pub queue_depth: usize,
    #[reflect(ignore)]
    queue: VecDeque<Entity>,
    #[reflect(ignore)]
    queued: HashSet<Entity>,
}

impl Default for GoapPlannerScheduler {
    fn default() -> Self {
        Self {
            max_agents_per_frame: 8,
            queue_depth: 0,
            queue: VecDeque::new(),
            queued: HashSet::default(),
        }
    }
}

impl GoapPlannerScheduler {
    pub fn enqueue(&mut self, entity: Entity) {
        if self.queued.insert(entity) {
            self.queue.push_back(entity);
            self.queue_depth = self.queue.len();
        }
    }

    pub fn dequeue(&mut self) -> Option<Entity> {
        let entity = self.queue.pop_front();
        if let Some(entity) = entity {
            self.queued.remove(&entity);
        }
        self.queue_depth = self.queue.len();
        entity
    }

    pub fn remove(&mut self, entity: Entity) {
        if self.queued.remove(&entity) {
            self.queue.retain(|candidate| *candidate != entity);
            self.queue_depth = self.queue.len();
        }
    }
}

#[derive(Resource, Default)]
pub struct GoapMessageCursors {
    pub action_reports: MessageCursor<ActionExecutionReport>,
    pub invalidate_agents: MessageCursor<InvalidateGoapAgent>,
    pub invalidate_local_sensors: MessageCursor<InvalidateLocalSensors>,
    pub invalidate_global_sensors: MessageCursor<InvalidateGlobalSensors>,
}
