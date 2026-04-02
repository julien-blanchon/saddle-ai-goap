use bevy::prelude::*;

use crate::planner::GoapPlannerLimits;
use crate::world_state::{
    FactCondition, FactEffect, GoapWorldState, TargetToken, WorldKeyId, WorldStateSchema,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub struct GoapDomainId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub struct GoalId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub struct ActionId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub struct SensorId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect)]
pub struct HookKey(pub String);

impl HookKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for HookKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for HookKey {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum SensorScope {
    Local,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub struct SensorInterval {
    pub seconds: f32,
    pub phase_offset: f32,
}

impl SensorInterval {
    pub fn every(seconds: f32) -> Self {
        Self {
            seconds,
            phase_offset: 0.0,
        }
    }

    pub fn with_phase_offset(mut self, phase_offset: f32) -> Self {
        self.phase_offset = phase_offset;
        self
    }
}

impl Default for SensorInterval {
    fn default() -> Self {
        Self::every(0.25)
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct SensorDefinition {
    pub id: SensorId,
    pub name: String,
    pub scope: SensorScope,
    pub handler: HookKey,
    pub interval: SensorInterval,
    pub outputs: Vec<WorldKeyId>,
}

impl SensorDefinition {
    pub fn new(
        id: SensorId,
        name: impl Into<String>,
        scope: SensorScope,
        handler: impl Into<HookKey>,
        outputs: impl IntoIterator<Item = WorldKeyId>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            scope,
            handler: handler.into(),
            interval: SensorInterval::default(),
            outputs: outputs.into_iter().collect(),
        }
    }

    pub fn with_interval(mut self, interval: SensorInterval) -> Self {
        self.interval = interval;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct GoalDefinition {
    pub id: GoalId,
    pub name: String,
    pub desired_state: Vec<FactCondition>,
    pub priority: i32,
    pub relevance: Option<HookKey>,
    pub validator: Option<HookKey>,
    pub completion: Option<HookKey>,
}

impl GoalDefinition {
    pub fn new(id: GoalId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            desired_state: Vec::new(),
            priority: 0,
            relevance: None,
            validator: None,
            completion: None,
        }
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_desired_state(
        mut self,
        desired_state: impl IntoIterator<Item = FactCondition>,
    ) -> Self {
        self.desired_state = desired_state.into_iter().collect();
        self
    }

    pub fn with_relevance(mut self, hook: impl Into<HookKey>) -> Self {
        self.relevance = Some(hook.into());
        self
    }

    pub fn with_validator(mut self, hook: impl Into<HookKey>) -> Self {
        self.validator = Some(hook.into());
        self
    }

    pub fn with_completion(mut self, hook: impl Into<HookKey>) -> Self {
        self.completion = Some(hook.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct ActionTargetSpec {
    pub slot: String,
    pub provider: HookKey,
}

impl ActionTargetSpec {
    pub fn new(slot: impl Into<String>, provider: impl Into<HookKey>) -> Self {
        Self {
            slot: slot.into(),
            provider: provider.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct ActionDefinition {
    pub id: ActionId,
    pub name: String,
    pub executor: HookKey,
    pub preconditions: Vec<FactCondition>,
    pub effects: Vec<FactEffect>,
    pub base_cost: u32,
    pub dynamic_cost: Option<HookKey>,
    pub context_validator: Option<HookKey>,
    pub target: Option<ActionTargetSpec>,
}

impl ActionDefinition {
    pub fn new(id: ActionId, name: impl Into<String>, executor: impl Into<HookKey>) -> Self {
        Self {
            id,
            name: name.into(),
            executor: executor.into(),
            preconditions: Vec::new(),
            effects: Vec::new(),
            base_cost: 1,
            dynamic_cost: None,
            context_validator: None,
            target: None,
        }
    }

    pub fn with_preconditions(
        mut self,
        preconditions: impl IntoIterator<Item = FactCondition>,
    ) -> Self {
        self.preconditions = preconditions.into_iter().collect();
        self
    }

    pub fn with_effects(mut self, effects: impl IntoIterator<Item = FactEffect>) -> Self {
        self.effects = effects.into_iter().collect();
        self
    }

    pub fn with_base_cost(mut self, cost: u32) -> Self {
        self.base_cost = cost.max(1);
        self
    }

    pub fn with_dynamic_cost(mut self, hook: impl Into<HookKey>) -> Self {
        self.dynamic_cost = Some(hook.into());
        self
    }

    pub fn with_context_validator(mut self, hook: impl Into<HookKey>) -> Self {
        self.context_validator = Some(hook.into());
        self
    }

    pub fn with_target(mut self, slot: impl Into<String>, provider: impl Into<HookKey>) -> Self {
        self.target = Some(ActionTargetSpec::new(slot, provider));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct GoapDomainDefinition {
    pub name: String,
    pub schema: WorldStateSchema,
    pub default_planner_limits: GoapPlannerLimits,
    pub goals: Vec<GoalDefinition>,
    pub actions: Vec<ActionDefinition>,
    pub local_sensors: Vec<SensorDefinition>,
    pub global_sensors: Vec<SensorDefinition>,
}

impl GoapDomainDefinition {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: WorldStateSchema::default(),
            default_planner_limits: GoapPlannerLimits::default(),
            goals: Vec::new(),
            actions: Vec::new(),
            local_sensors: Vec::new(),
            global_sensors: Vec::new(),
        }
    }

    pub fn with_default_limits(mut self, limits: GoapPlannerLimits) -> Self {
        self.default_planner_limits = limits;
        self
    }

    pub fn add_bool_key(
        &mut self,
        name: impl Into<String>,
        description: impl Into<Option<String>>,
        default_value: Option<bool>,
    ) -> WorldKeyId {
        self.schema.add_bool_key(name, description, default_value)
    }

    pub fn add_int_key(
        &mut self,
        name: impl Into<String>,
        description: impl Into<Option<String>>,
        default_value: Option<i32>,
    ) -> WorldKeyId {
        self.schema.add_int_key(name, description, default_value)
    }

    pub fn add_target_key(
        &mut self,
        name: impl Into<String>,
        description: impl Into<Option<String>>,
        default_value: Option<TargetToken>,
    ) -> WorldKeyId {
        self.schema.add_target_key(name, description, default_value)
    }

    pub fn add_goal(&mut self, mut goal: GoalDefinition) -> GoalId {
        let id = GoalId(self.goals.len());
        goal.id = id;
        self.goals.push(goal);
        id
    }

    pub fn add_action(&mut self, mut action: ActionDefinition) -> ActionId {
        let id = ActionId(self.actions.len());
        action.id = id;
        self.actions.push(action);
        id
    }

    pub fn add_local_sensor(&mut self, mut sensor: SensorDefinition) -> SensorId {
        let id = SensorId(self.local_sensors.len());
        sensor.id = id;
        sensor.scope = SensorScope::Local;
        self.local_sensors.push(sensor);
        id
    }

    pub fn add_global_sensor(&mut self, mut sensor: SensorDefinition) -> SensorId {
        let id = SensorId(self.global_sensors.len());
        sensor.id = id;
        sensor.scope = SensorScope::Global;
        self.global_sensors.push(sensor);
        id
    }

    pub fn default_state(&self) -> GoapWorldState {
        self.schema.default_state()
    }
}
