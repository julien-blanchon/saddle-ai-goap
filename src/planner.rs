use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::definitions::{ActionId, GoalId, HookKey};
use crate::world_state::{FactCondition, FactEffect, GoapWorldState, TargetToken};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub struct GoapPlannerLimits {
    pub max_node_expansions: u32,
    pub max_plan_length: usize,
    pub max_expansions_per_step: u32,
}

impl Default for GoapPlannerLimits {
    fn default() -> Self {
        Self {
            max_node_expansions: 256,
            max_plan_length: 8,
            max_expansions_per_step: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct SelectedGoal {
    pub id: GoalId,
    pub name: String,
    pub priority: i32,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct TargetCandidate {
    pub token: TargetToken,
    pub label: String,
    pub cost_bias: u32,
    pub debug_position: Option<Vec3>,
}

impl TargetCandidate {
    pub fn new(token: TargetToken, label: impl Into<String>) -> Self {
        Self {
            token,
            label: label.into(),
            cost_bias: 0,
            debug_position: None,
        }
    }

    pub fn with_cost_bias(mut self, cost_bias: u32) -> Self {
        self.cost_bias = cost_bias;
        self
    }

    pub fn with_debug_position(mut self, debug_position: Vec3) -> Self {
        self.debug_position = Some(debug_position);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct PreparedActionVariant {
    pub action_id: ActionId,
    pub action_name: String,
    pub executor: HookKey,
    pub preconditions: Vec<FactCondition>,
    pub effects: Vec<FactEffect>,
    pub cost: u32,
    pub target_slot: Option<String>,
    pub target: Option<TargetCandidate>,
    pub sort_index: usize,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct GoapPlanStep {
    pub action_id: ActionId,
    pub action_name: String,
    pub executor: HookKey,
    pub cost: u32,
    pub target_slot: Option<String>,
    pub target: Option<TargetCandidate>,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct GoapPlanDraft {
    pub goal: SelectedGoal,
    pub steps: Vec<GoapPlanStep>,
    pub total_cost: u32,
    pub expansions: u32,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct PlanningProblem {
    pub initial_state: GoapWorldState,
    pub state_revision: u64,
    pub goal: SelectedGoal,
    pub desired_state: Vec<FactCondition>,
    pub actions: Vec<PreparedActionVariant>,
    pub limits: GoapPlannerLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum PlanningFailureReason {
    NoPlan,
    MaxNodeExpansions,
    MaxPlanLength,
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub enum PlanningStepOutcome {
    InProgress {
        expansions: u32,
        total_expansions: u32,
    },
    Success(GoapPlanDraft),
    Failure {
        reason: PlanningFailureReason,
        expansions: u32,
        total_expansions: u32,
    },
}

#[derive(Debug, Clone)]
struct SearchNode {
    state: GoapWorldState,
    parent: Option<usize>,
    via_action: Option<usize>,
    path_cost: u32,
    depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QueueEntry {
    total_cost: u32,
    path_cost: u32,
    depth: usize,
    action_sort_index: usize,
    insertion_order: u64,
    node_index: usize,
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .total_cost
            .cmp(&self.total_cost)
            .then_with(|| other.path_cost.cmp(&self.path_cost))
            .then_with(|| other.depth.cmp(&self.depth))
            .then_with(|| other.action_sort_index.cmp(&self.action_sort_index))
            .then_with(|| other.insertion_order.cmp(&self.insertion_order))
    }
}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
pub struct PlanningSession {
    problem: PlanningProblem,
    nodes: Vec<SearchNode>,
    open: BinaryHeap<QueueEntry>,
    best_costs: HashMap<GoapWorldState, u32>,
    total_expansions: u32,
    hit_plan_length: bool,
    next_insertion_order: u64,
}

impl PlanningSession {
    pub fn new(problem: PlanningProblem) -> Self {
        let root = SearchNode {
            state: problem.initial_state.clone(),
            parent: None,
            via_action: None,
            path_cost: 0,
            depth: 0,
        };
        let mut best_costs = HashMap::default();
        best_costs.insert(problem.initial_state.clone(), 0);
        let mut open = BinaryHeap::new();
        open.push(QueueEntry {
            total_cost: heuristic(&problem.initial_state, &problem.desired_state),
            path_cost: 0,
            depth: 0,
            action_sort_index: 0,
            insertion_order: 0,
            node_index: 0,
        });

        Self {
            problem,
            nodes: vec![root],
            open,
            best_costs,
            total_expansions: 0,
            hit_plan_length: false,
            next_insertion_order: 1,
        }
    }

    pub fn total_expansions(&self) -> u32 {
        self.total_expansions
    }

    pub fn source_revision(&self) -> u64 {
        self.problem.state_revision
    }

    pub fn problem(&self) -> &PlanningProblem {
        &self.problem
    }

    pub fn step(&mut self, budget: u32) -> PlanningStepOutcome {
        let mut step_expansions = 0;
        let budget = budget.max(1);

        while step_expansions < budget {
            let Some(entry) = self.open.pop() else {
                let reason = if self.hit_plan_length {
                    PlanningFailureReason::MaxPlanLength
                } else {
                    PlanningFailureReason::NoPlan
                };
                return PlanningStepOutcome::Failure {
                    reason,
                    expansions: step_expansions,
                    total_expansions: self.total_expansions,
                };
            };

            let node = &self.nodes[entry.node_index];
            if goal_satisfied(&node.state, &self.problem.desired_state) {
                return PlanningStepOutcome::Success(self.reconstruct_plan(entry.node_index));
            }

            if self.total_expansions >= self.problem.limits.max_node_expansions {
                return PlanningStepOutcome::Failure {
                    reason: PlanningFailureReason::MaxNodeExpansions,
                    expansions: step_expansions,
                    total_expansions: self.total_expansions,
                };
            }

            if node.depth >= self.problem.limits.max_plan_length {
                self.hit_plan_length = true;
                continue;
            }

            step_expansions += 1;
            self.total_expansions += 1;

            let current_state = node.state.clone();
            let current_cost = node.path_cost;
            let current_depth = node.depth;

            for (variant_index, action) in self.problem.actions.iter().enumerate() {
                if !action
                    .preconditions
                    .iter()
                    .all(|condition| condition.matches(&current_state))
                {
                    continue;
                }

                let mut next_state = current_state.clone();
                for effect in &action.effects {
                    effect.apply(&mut next_state);
                }

                let next_cost = current_cost + action.cost.max(1);
                if self
                    .best_costs
                    .get(&next_state)
                    .is_some_and(|best| *best <= next_cost)
                {
                    continue;
                }

                self.best_costs.insert(next_state.clone(), next_cost);
                let node_index = self.nodes.len();
                self.nodes.push(SearchNode {
                    state: next_state.clone(),
                    parent: Some(entry.node_index),
                    via_action: Some(variant_index),
                    path_cost: next_cost,
                    depth: current_depth + 1,
                });

                self.open.push(QueueEntry {
                    total_cost: next_cost + heuristic(&next_state, &self.problem.desired_state),
                    path_cost: next_cost,
                    depth: current_depth + 1,
                    action_sort_index: action.sort_index,
                    insertion_order: self.next_insertion_order,
                    node_index,
                });
                self.next_insertion_order += 1;
            }
        }

        PlanningStepOutcome::InProgress {
            expansions: step_expansions,
            total_expansions: self.total_expansions,
        }
    }

    fn reconstruct_plan(&self, mut node_index: usize) -> GoapPlanDraft {
        let total_cost = self.nodes[node_index].path_cost;
        let mut steps = Vec::new();
        while let Some(parent) = self.nodes[node_index].parent {
            let action_index = self.nodes[node_index]
                .via_action
                .expect("child nodes should always reference an action");
            let action = &self.problem.actions[action_index];
            steps.push(GoapPlanStep {
                action_id: action.action_id,
                action_name: action.action_name.clone(),
                executor: action.executor.clone(),
                cost: action.cost,
                target_slot: action.target_slot.clone(),
                target: action.target.clone(),
            });
            node_index = parent;
        }
        steps.reverse();

        GoapPlanDraft {
            goal: self.problem.goal.clone(),
            total_cost,
            steps,
            expansions: self.total_expansions,
        }
    }
}

fn goal_satisfied(state: &GoapWorldState, desired_state: &[FactCondition]) -> bool {
    desired_state
        .iter()
        .all(|condition| condition.matches(state))
}

fn heuristic(state: &GoapWorldState, desired_state: &[FactCondition]) -> u32 {
    desired_state
        .iter()
        .filter(|condition| !condition.matches(state))
        .count() as u32
}

#[cfg(test)]
#[path = "planner_tests.rs"]
mod tests;
