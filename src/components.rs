use bevy::prelude::*;

use crate::definitions::{ActionId, GoalId, GoapDomainId, SensorDefinition, SensorId};
use crate::planner::{
    GoapPlanDraft, GoapPlanStep, GoapPlannerLimits, PlanningProblem, PlanningSession, SelectedGoal,
    TargetCandidate,
};
use crate::world_state::{GoapWorldState, WorldKeyId};

#[derive(Debug, Clone, PartialEq, Eq, Reflect)]
pub enum PlannerStatus {
    Inactive,
    Idle,
    Sensing,
    SelectingGoal,
    QueuedForPlanning,
    Planning,
    Dispatching,
    WaitingOnAction,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Reflect)]
pub enum PlanInvalidationReason {
    RequiredFactChanged { key: WorldKeyId },
    SensorRefresh,
    TargetInvalidated,
    ActionFailed { reason: String },
    HigherPriorityGoal,
    GoalCompleted,
    GoalNoLongerValid,
    Manual { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Reflect)]
pub enum ActiveActionStatus {
    Dispatched,
    Running,
    Waiting,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct GoapPlan {
    pub goal: SelectedGoal,
    pub steps: Vec<GoapPlanStep>,
    pub cursor: usize,
    pub total_cost: u32,
    pub expansions: u32,
    pub built_from_revision: u64,
}

impl GoapPlan {
    pub fn from_draft(draft: GoapPlanDraft, built_from_revision: u64) -> Self {
        Self {
            goal: draft.goal,
            steps: draft.steps,
            cursor: 0,
            total_cost: draft.total_cost,
            expansions: draft.expansions,
            built_from_revision,
        }
    }

    pub fn current_step(&self) -> Option<&GoapPlanStep> {
        self.steps.get(self.cursor)
    }

    pub fn advance(&mut self) {
        self.cursor += 1;
    }

    pub fn finished(&self) -> bool {
        self.cursor >= self.steps.len()
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct ActiveAction {
    pub ticket: u64,
    pub action_id: ActionId,
    pub action_name: String,
    pub target_slot: Option<String>,
    pub target: Option<TargetCandidate>,
    pub status: ActiveActionStatus,
    pub note: Option<String>,
    pub started_at_seconds: f32,
}

impl ActiveAction {
    pub fn from_step(ticket: u64, step: &GoapPlanStep, started_at_seconds: f32) -> Self {
        Self {
            ticket,
            action_id: step.action_id,
            action_name: step.action_name.clone(),
            target_slot: step.target_slot.clone(),
            target: step.target.clone(),
            status: ActiveActionStatus::Dispatched,
            note: None,
            started_at_seconds,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct SensorRuntimeInfo {
    pub sensor_id: SensorId,
    pub name: String,
    pub next_due_seconds: f32,
    pub last_run_seconds: Option<f32>,
    pub run_count: u64,
    pub last_note: Option<String>,
}

impl SensorRuntimeInfo {
    pub fn from_definition(definition: &SensorDefinition) -> Self {
        Self {
            sensor_id: definition.id,
            name: definition.name.clone(),
            next_due_seconds: definition.interval.phase_offset,
            last_run_seconds: None,
            run_count: 0,
            last_note: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Reflect, Default)]
pub struct GoapCounters {
    pub sensor_refreshes: u64,
    pub replans: u64,
    pub cached_plan_hits: u64,
    pub invalidations: u64,
    pub dispatched_actions: u64,
    pub completed_plans: u64,
    pub failed_plans: u64,
    pub goal_switches: u64,
    pub total_expansions: u64,
    pub last_expansions: u32,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct CachedPlanEntry {
    pub problem: PlanningProblem,
    pub draft: GoapPlanDraft,
    pub hit_count: u64,
}

#[derive(Component, Debug, Clone, PartialEq, Reflect)]
#[reflect(Component)]
pub struct GoapAgent {
    pub domain: GoapDomainId,
    pub config: GoapAgentConfig,
}

impl GoapAgent {
    pub fn new(domain: GoapDomainId) -> Self {
        Self {
            domain,
            config: GoapAgentConfig::default(),
        }
    }

    pub fn with_config(mut self, config: GoapAgentConfig) -> Self {
        self.config = config;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct GoapAgentConfig {
    pub planner_limits: Option<GoapPlannerLimits>,
    pub plan_cache_capacity: usize,
    pub preempt_on_better_goal: bool,
    pub goal_switch_margin: f32,
    pub replan_on_sensed_state_change: bool,
}

impl GoapAgentConfig {
    pub fn with_planner_limits(mut self, limits: GoapPlannerLimits) -> Self {
        self.planner_limits = Some(limits);
        self
    }

    pub fn with_plan_cache_capacity(mut self, plan_cache_capacity: usize) -> Self {
        self.plan_cache_capacity = plan_cache_capacity;
        self
    }

    pub fn resolve_planner_limits(&self, domain_default: GoapPlannerLimits) -> GoapPlannerLimits {
        self.planner_limits.unwrap_or(domain_default)
    }
}

impl Default for GoapAgentConfig {
    fn default() -> Self {
        Self {
            planner_limits: None,
            plan_cache_capacity: 8,
            preempt_on_better_goal: true,
            goal_switch_margin: 0.25,
            replan_on_sensed_state_change: true,
        }
    }
}

#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct GoapRuntime {
    pub status: PlannerStatus,
    pub sensed_state: GoapWorldState,
    pub local_state: GoapWorldState,
    pub current_goal: Option<SelectedGoal>,
    pub last_failed_goal: Option<GoalId>,
    pub last_failed_revision: Option<u64>,
    pub current_plan: Option<GoapPlan>,
    pub active_action: Option<ActiveAction>,
    pub last_invalidation_reason: Option<PlanInvalidationReason>,
    pub counters: GoapCounters,
    pub local_sensors: Vec<SensorRuntimeInfo>,
    pub global_sensors: Vec<SensorRuntimeInfo>,
    pub sensor_revision: u64,
    pub observed_global_revision: u64,
    pub next_action_ticket: u64,
    pub plan_cache: Vec<CachedPlanEntry>,
    #[reflect(ignore)]
    pub planning_session: Option<PlanningSession>,
}

impl GoapRuntime {
    pub fn new(
        sensed_state: GoapWorldState,
        local_sensors: Vec<SensorRuntimeInfo>,
        global_sensors: Vec<SensorRuntimeInfo>,
        observed_global_revision: u64,
    ) -> Self {
        Self {
            status: PlannerStatus::Idle,
            sensed_state,
            local_state: GoapWorldState::default(),
            current_goal: None,
            last_failed_goal: None,
            last_failed_revision: None,
            current_plan: None,
            active_action: None,
            last_invalidation_reason: None,
            counters: GoapCounters::default(),
            local_sensors,
            global_sensors,
            sensor_revision: 0,
            observed_global_revision,
            next_action_ticket: 1,
            plan_cache: Vec::new(),
            planning_session: None,
        }
    }

    pub fn cached_plan(&mut self, problem: &PlanningProblem) -> Option<GoapPlanDraft> {
        let index = self
            .plan_cache
            .iter()
            .position(|entry| entry.problem == *problem)?;
        let mut entry = self.plan_cache.remove(index);
        entry.hit_count = entry.hit_count.saturating_add(1);
        let draft = entry.draft.clone();
        self.plan_cache.insert(0, entry);
        Some(draft)
    }

    pub fn store_cached_plan(
        &mut self,
        capacity: usize,
        problem: PlanningProblem,
        draft: GoapPlanDraft,
    ) {
        if capacity == 0 {
            self.plan_cache.clear();
            return;
        }

        if let Some(index) = self
            .plan_cache
            .iter()
            .position(|entry| entry.problem == problem)
        {
            self.plan_cache.remove(index);
        }

        self.plan_cache.insert(
            0,
            CachedPlanEntry {
                problem,
                draft,
                hit_count: 0,
            },
        );
        self.plan_cache.truncate(capacity);
    }
}
