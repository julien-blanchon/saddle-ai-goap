# Debugging

Runtime visibility is a first-class requirement for `saddle-ai-goap`.

## What To Inspect

At minimum, every live agent exposes:

- current goal
- planner status
- plan chain
- active target bindings
- last invalidation reason
- deferred invalidation (if set, explains why a replan is pending behind a non-interruptible action)
- sensed symbolic state
- runtime counters
- reserved targets

These are available through `GoapDebugSnapshot` and the full `GoapRuntime` component.

## Messages

High-value runtime messages:

- `GoalChanged`
- `PlanStarted`
- `PlanCompleted`
- `PlanFailed`
- `PlanInvalidated`
- `ActionDispatched`
- `ActionCancelled`
- `ActionExecutionReport`

These are intentionally coarse-grained. The runtime does not emit per-frame chatter by default.

## BRP-Friendly Types

Useful BRP targets:

- `saddle_ai_goap::components::GoapAgent`
- `saddle_ai_goap::components::GoapRuntime`
- `saddle_ai_goap::debug::GoapDebugSnapshot`
- `saddle_ai_goap::resources::GoapPlannerScheduler`
- `saddle_ai_goap::resources::GoapGlobalSensorCache`
- `saddle_ai_goap::reservations::GoapReservationMap`

The debug snapshot is the fastest way to answer "what is this agent trying to do right now?" The full runtime component is better when you need sensor timing, counters, or the active action ticket.

## Crate-Local Lab

The recommended live debugging target is `saddle-ai-goap-lab`.

```bash
cargo run -p saddle-ai-goap-lab
```

### E2E scenarios

```bash
cargo run -p saddle-ai-goap-lab --features e2e -- smoke_launch
cargo run -p saddle-ai-goap-lab --features e2e -- goap_smoke
cargo run -p saddle-ai-goap-lab --features e2e -- goap_replan
cargo run -p saddle-ai-goap-lab --features e2e -- goap_worker_cycle
```

Scenario intent:

- `smoke_launch`
  planner boots and both agents acquire a plan
- `goap_smoke`
  alias behavior for the default planner overlay capture
- `goap_replan`
  guard loses a target, invalidates, replans, and resolves the fallback
- `goap_worker_cycle`
  worker loses workstation availability, invalidates, replans, and eventually delivers

Each scenario combines screenshots with at least one hard runtime assertion.

## BRP Workflow

```bash
uv run --active --project .codex/skills/bevy-brp/script brp app launch saddle-ai-goap-lab
uv run --active --project .codex/skills/bevy-brp/script brp world query bevy_ecs::name::Name
uv run --active --project .codex/skills/bevy-brp/script brp world query saddle_ai_goap::debug::GoapDebugSnapshot
uv run --active --project .codex/skills/bevy-brp/script brp world query saddle_ai_goap::components::GoapRuntime
uv run --active --project .codex/skills/bevy-brp/script brp resource get saddle_ai_goap::resources::GoapPlannerScheduler
uv run --active --project .codex/skills/bevy-brp/script brp resource get saddle_ai_goap::resources::GoapGlobalSensorCache
uv run --active --project .codex/skills/bevy-brp/script brp extras screenshot /tmp/saddle_ai_goap_lab.png
uv run --active --project .codex/skills/bevy-brp/script brp extras shutdown
```

Questions these commands answer quickly:

- Which entities are planners?
- Which goal is active?
- Which action is currently dispatched?
- Which sensor cache revision is live?
- Is the planner queue backing up?
- Which targets are reserved and by whom?
- Is a soft invalidation deferred behind a non-interruptible action?

## Reading The Snapshot

`GoapDebugSnapshot` fields:

- `current_goal`
  current goal name or `None`
- `planner_status`
  human-readable `PlannerStatus`
- `plan_chain`
  ordered plan with the current cursor marked
- `active_targets`
  active bound targets rendered as `slot => label`
- `last_invalidation`
  last explicit invalidation reason
- `sensed_state`
  compact symbolic state dump
- `counters`
  aggregate plan, invalidation, and expansion counters

## Common Failure Modes

### Agent never plans

Check:

- the entity has both `GoapAgent` and `GoapRuntime`
- the selected goal is valid
- at least one action can satisfy the goal
- `GoapPlannerScheduler.queue_depth` is not stuck behind a tiny per-frame budget

### Plan keeps invalidating

Check:

- `last_invalidation`
- sensor intervals and invalidation messages
- whether `replan_on_sensed_state_change` is too aggressive for the domain
- whether target providers are returning unstable candidate sets

### Planner keeps failing the same goal

Check:

- whether the current goal is still valid when no action can satisfy it
- whether sensors are actually changing revision after the world changes
- whether the failed-plan retry gate is intentionally holding the goal until a relevant fact changes

### Action dispatches but nothing happens

Check:

- that a game-side system reads `ActionDispatched`
- that it replies with `ActionExecutionReport`
- that the action ticket in the report matches the active action ticket

### Sensor cache looks stale

Check:

- `GoapGlobalSensorCache` revisions
- per-sensor `next_due_seconds` and `last_run_seconds`
- whether the game should send `InvalidateLocalSensors` or `InvalidateGlobalSensors`

### Action never completes / replan keeps deferring

Check:

- `ActiveAction.interruptible` is `false` and holding completion
- `GoapRuntime.deferred_invalidation` is set (indicates a pending soft invalidation)
- whether the action executor has legitimately not reported `Success` or `Failure` yet
- sensor intervals — frequent refreshes on non-interruptible actions queue deferred invalidations that pile up

### Two agents target the same entity

Check:

- whether the domain has a `reservation_policy` set
- `GoapReservationMap` for current reservations per domain
- whether `cost_penalty` is high enough to dissuade competing agents
- whether `hard_block` should be enabled instead
