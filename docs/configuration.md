# Configuration

This document covers every public tuning surface in `saddle-ai-goap`.

## `GoapPlugin`

```rust
GoapPlugin::new(activate_schedule, deactivate_schedule, update_schedule)
GoapPlugin::always_on(update_schedule)
```

- `activate_schedule`
  schedule that initializes runtime components for newly active agents
- `deactivate_schedule`
  schedule that removes runtime state when planners should go offline
- `update_schedule`
  schedule that runs `GoapSystems::{Sense, SelectGoal, Plan, Dispatch, Monitor, Cleanup, Debug}`

Use `always_on` for standalone examples or apps where planners never deactivate.

## `GoapDomainDefinition::with_default_limits`

Sets the default `GoapPlannerLimits` for every agent in the domain.

- use this when the whole domain should share the same planning budget
- agents can still opt out with `GoapAgentConfig::with_planner_limits(...)`

## `GoapAgentConfig`

Per-agent planner policy.

### `planner_limits: Option<GoapPlannerLimits>`

Default: `None`

- `None`
  use the domain's `with_default_limits(...)` value
- `Some(limits)`
  override the domain default on this agent only

Prefer the builder:

```rust
GoapAgentConfig::default().with_planner_limits(custom_limits)
```

### `preempt_on_better_goal: bool`

Default: `true`

- `true`
  a higher-scored goal can interrupt the current goal
- `false`
  the agent keeps the current goal until it completes or becomes invalid

### `goal_switch_margin: f32`

Default: `0.25`

The new goal must beat the current goal by at least this much score before preemption happens. This reduces goal thrashing when two goals are nearly tied.

### `replan_on_sensed_state_change: bool`

Default: `true`

- `true`
  when the agent is between actions and a newer sensor revision exists than the current plan or in-flight planning session, the runtime invalidates the stale work with `PlanInvalidationReason::SensorRefresh`
- `false`
  the runtime keeps the current plan unless a required fact changes, a target disappears, execution fails, or a better goal wins

## Failed-plan retry gate

`saddle-ai-goap` deliberately avoids retrying the exact same failed plan every frame.

- after `PlanFailed`, the runtime records the current goal and sensor revision
- while that goal and sensor revision stay unchanged, `GoapSystems::SelectGoal` will not immediately queue the same plan again
- retries resume automatically when sensed state changes, the goal changes, or an explicit invalidation clears the stale failure record

This keeps impossible goals from hammering the planner queue and inflating failure metrics.

## `GoapPlannerLimits`

Per-plan search limits.

### `max_node_expansions: u32`

Default: `256`

Hard guardrail for the full search. When reached, planning fails with `PlanningFailureReason::MaxNodeExpansions`.

### `max_plan_length: usize`

Default: `8`

Maximum number of symbolic actions allowed in a plan. Use this to stop runaway search in noisy action spaces.

### `max_expansions_per_step: u32`

Default: `64`

Per-frame search budget for incremental planning. Lower values spread work across more frames; higher values reduce planning latency but can spike frame time.

## `SensorInterval`

Per-sensor polling cadence.

### `seconds: f32`

Default via `SensorInterval::default()`: `0.25`

- `0.0`
  refresh every planner frame
- `> 0.0`
  refresh only after that amount of elapsed time

### `phase_offset: f32`

Default: `0.0`

Offsets the first scheduled refresh. Useful for staggering large sensor populations.

## `GoapPlannerScheduler`

Shared planner queue resource.

### `max_agents_per_frame: usize`

Default: `8`

Number of agents whose planning work may advance in one frame. Lower it when you have many agents and want tighter frame-time control.

### `queue_depth: usize`

Read-only runtime metric that mirrors the current queue length. The queue itself is internal; `queue_depth` exists so BRP, overlays, and diagnostics can inspect pressure without exposing queue internals.

## Goal Definitions

### `priority: i32`

Base fixed-priority bias.

### `relevance: Option<HookKey>`

Optional dynamic score hook. The final score is:

```text
priority as f32 + hook_result
```

### `validator: Option<HookKey>`

Optional boolean hook used to reject a goal before planning.

### `completion: Option<HookKey>`

Optional boolean hook that overrides pure desired-state completion checks.

## Action Definitions

### `base_cost: u32`

Base symbolic action cost. The runtime clamps it to at least `1`.

### `dynamic_cost: Option<HookKey>`

Optional hook that adds a dynamic signed delta to `base_cost`.

### `context_validator: Option<HookKey>`

Optional hook used to reject an action variant, including target-bound variants, before planning.

### `target: Option<ActionTargetSpec>`

Optional target slot plus provider key. When present, the planner asks game code for candidate targets and expands one symbolic action variant per target.

## Debug Surface

### `GoapDebugSnapshot`

Always attached alongside `GoapRuntime`. Use it when you want a compact inspection surface instead of dumping the full runtime component.

### `GoapGlobalSensorCache`

Reflect-enabled resource holding domain-scoped symbolic caches and global sensor runtime info.

### `GoapRuntime`

Reflect-enabled component exposing the full runtime state, including local and global sensor timing, active action ticket, counters, and last invalidation reason.
