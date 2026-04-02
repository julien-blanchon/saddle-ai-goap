use bevy::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Reflect)]
pub struct GoapDebugEntry {
    pub key: String,
    pub value: String,
}

#[derive(Component, Debug, Clone, PartialEq, Eq, Reflect, Default)]
#[reflect(Component)]
pub struct GoapDebugSnapshot {
    pub current_goal: Option<String>,
    pub planner_status: String,
    pub plan_chain: Vec<String>,
    pub active_targets: Vec<String>,
    pub last_invalidation: Option<String>,
    pub sensed_state: Vec<GoapDebugEntry>,
    pub counters: Vec<GoapDebugEntry>,
}
