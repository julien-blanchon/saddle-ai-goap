use bevy::ecs::intern::Interned;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

pub mod assets;
pub mod components;
pub mod debug;
pub mod definitions;
pub mod execution;
pub mod messages;
pub mod planner;
pub mod resources;
pub mod systems;
pub mod world_state;

pub use assets::{GoapDomainAsset, GoapDomainAssetLoader, GoapDomainAssetLoaderError};
pub use components::{
    ActiveAction, ActiveActionStatus, CachedPlanEntry, GoapAgent, GoapAgentConfig,
    GoapCounters, GoapPlan, GoapRuntime, PlanInvalidationReason, PlannerStatus,
    SensorRuntimeInfo,
};
pub use debug::{GoapDebugEntry, GoapDebugSnapshot};
pub use definitions::{
    ActionDefinition, ActionId, ActionTargetSpec, GoalDefinition, GoalId, GoapDomainDefinition,
    GoapDomainId, HookKey, SensorDefinition, SensorId, SensorInterval, SensorScope,
};
pub use execution::{
    ActionEvaluationContext, GlobalSensorContext, GoalHookContext, LocalSensorContext,
    SensorOutput, TargetProviderContext,
};
pub use messages::{
    ActionCancelled, ActionDispatched, ActionExecutionReport, ActionExecutionStatus, GoalChanged,
    InvalidateGlobalSensors, InvalidateGoapAgent, InvalidateLocalSensors, PlanCompleted,
    PlanFailed, PlanInvalidated, PlanStarted,
};
pub use planner::{
    GoapPlanDraft, GoapPlanStep, GoapPlannerLimits, PlanningFailureReason, PlanningProblem,
    PlanningSession, PlanningStepOutcome, PreparedActionVariant, SelectedGoal, TargetCandidate,
};
pub use resources::{
    DomainGlobalCache, GoapGlobalSensorCache, GoapHooks, GoapLibrary, GoapPlannerScheduler,
};
pub use world_state::{
    FactComparison, FactCondition, FactEffect, FactPatch, FactValue, FactValueType, GoapWorldState,
    TargetToken, WorldKeyDefinition, WorldKeyId, WorldStateSchema,
};

#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum GoapSystems {
    Sense,
    SelectGoal,
    Plan,
    Dispatch,
    Monitor,
    Cleanup,
    Debug,
}

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct NeverDeactivateSchedule;

pub struct GoapPlugin {
    pub activate_schedule: Interned<dyn ScheduleLabel>,
    pub deactivate_schedule: Interned<dyn ScheduleLabel>,
    pub update_schedule: Interned<dyn ScheduleLabel>,
}

impl GoapPlugin {
    pub fn new(
        activate_schedule: impl ScheduleLabel,
        deactivate_schedule: impl ScheduleLabel,
        update_schedule: impl ScheduleLabel,
    ) -> Self {
        Self {
            activate_schedule: activate_schedule.intern(),
            deactivate_schedule: deactivate_schedule.intern(),
            update_schedule: update_schedule.intern(),
        }
    }

    pub fn always_on(update_schedule: impl ScheduleLabel) -> Self {
        Self::new(PostStartup, NeverDeactivateSchedule, update_schedule)
    }
}

impl Default for GoapPlugin {
    fn default() -> Self {
        Self::always_on(Update)
    }
}

impl Plugin for GoapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GoapLibrary>()
            .init_resource::<GoapHooks>()
            .init_resource::<GoapPlannerScheduler>()
            .init_resource::<GoapGlobalSensorCache>()
            .init_resource::<resources::GoapMessageCursors>()
            .init_asset::<GoapDomainAsset>()
            .register_asset_loader(GoapDomainAssetLoader)
            .add_message::<ActionExecutionReport>()
            .add_message::<InvalidateGoapAgent>()
            .add_message::<InvalidateLocalSensors>()
            .add_message::<InvalidateGlobalSensors>()
            .add_message::<GoalChanged>()
            .add_message::<PlanStarted>()
            .add_message::<PlanCompleted>()
            .add_message::<PlanFailed>()
            .add_message::<PlanInvalidated>()
            .add_message::<ActionDispatched>()
            .add_message::<ActionCancelled>()
            .register_type::<ActiveAction>()
            .register_type::<ActiveActionStatus>()
            .register_type::<ActionCancelled>()
            .register_type::<ActionDefinition>()
            .register_type::<ActionDispatched>()
            .register_type::<ActionExecutionReport>()
            .register_type::<ActionExecutionStatus>()
            .register_type::<ActionId>()
            .register_type::<ActionTargetSpec>()
            .register_type::<GoapDomainAsset>()
            .register_type::<DomainGlobalCache>()
            .register_type::<FactComparison>()
            .register_type::<FactCondition>()
            .register_type::<FactEffect>()
            .register_type::<FactPatch>()
            .register_type::<FactValue>()
            .register_type::<FactValueType>()
            .register_type::<GlobalSensorContext>()
            .register_type::<GoalChanged>()
            .register_type::<GoalDefinition>()
            .register_type::<GoalHookContext>()
            .register_type::<GoalId>()
            .register_type::<GoapAgent>()
            .register_type::<GoapAgentConfig>()
            .register_type::<GoapCounters>()
            .register_type::<CachedPlanEntry>()
            .register_type::<GoapDebugEntry>()
            .register_type::<GoapDebugSnapshot>()
            .register_type::<GoapDomainDefinition>()
            .register_type::<GoapDomainId>()
            .register_type::<GoapGlobalSensorCache>()
            .register_type::<GoapLibrary>()
            .register_type::<GoapPlan>()
            .register_type::<GoapPlanDraft>()
            .register_type::<GoapPlanStep>()
            .register_type::<GoapPlannerLimits>()
            .register_type::<GoapPlannerScheduler>()
            .register_type::<GoapRuntime>()
            .register_type::<GoapWorldState>()
            .register_type::<HookKey>()
            .register_type::<InvalidateGlobalSensors>()
            .register_type::<InvalidateGoapAgent>()
            .register_type::<InvalidateLocalSensors>()
            .register_type::<LocalSensorContext>()
            .register_type::<PlanCompleted>()
            .register_type::<PlanFailed>()
            .register_type::<PlanInvalidated>()
            .register_type::<PlanInvalidationReason>()
            .register_type::<PlanStarted>()
            .register_type::<PlannerStatus>()
            .register_type::<PreparedActionVariant>()
            .register_type::<SelectedGoal>()
            .register_type::<SensorDefinition>()
            .register_type::<SensorId>()
            .register_type::<SensorInterval>()
            .register_type::<SensorOutput>()
            .register_type::<SensorRuntimeInfo>()
            .register_type::<SensorScope>()
            .register_type::<TargetCandidate>()
            .register_type::<TargetProviderContext>()
            .register_type::<TargetToken>()
            .register_type::<WorldKeyDefinition>()
            .register_type::<WorldKeyId>()
            .register_type::<WorldStateSchema>();

        app.add_systems(self.activate_schedule, systems::activate_agents);
        app.add_systems(self.deactivate_schedule, systems::deactivate_agents);
        app.add_systems(
            self.update_schedule,
            (
                systems::sense_agents.in_set(GoapSystems::Sense),
                systems::select_goals.in_set(GoapSystems::SelectGoal),
                systems::advance_planning.in_set(GoapSystems::Plan),
                systems::dispatch_actions.in_set(GoapSystems::Dispatch),
                systems::monitor_actions.in_set(GoapSystems::Monitor),
                systems::cleanup_agents.in_set(GoapSystems::Cleanup),
                systems::refresh_debug_snapshots.in_set(GoapSystems::Debug),
            )
                .chain(),
        );
    }
}
