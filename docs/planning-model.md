# Planning Model

`saddle-ai-goap` plans over symbolic facts, not raw ECS snapshots.

## World-State Model

The planner-visible state lives in `GoapWorldState` and is keyed by `WorldKeyId` entries from `WorldStateSchema`.

Supported first-class value types:

- `Bool`
- `Int`
- `Target`

Why no built-in float or position facts in v1:

- planner-visible state should stay compact and deterministic
- float-heavy symbolic states are awkward to hash and compare
- most spatial reasoning is better modeled as target selection plus context validation

If a domain needs location-aware planning, prefer:

- target providers returning `TargetCandidate`
- `TargetCandidate::debug_position` for overlays and tooling
- context validators or dynamic cost hooks for reachability, reservation, or travel heuristics

## Facts

### `FactCondition`

Supported comparisons:

- equality / inequality
- integer `>=`
- integer `<=`
- `IsSet`
- `IsUnset`

### `FactEffect`

Supported symbolic effects:

- set a fact
- add to an integer fact
- clear a fact

### `FactPatch`

Sensors emit `FactPatch` values to update symbolic memory.

## Goals

Goals describe desired outcomes, not scripts.

Each `GoalDefinition` contains:

- `desired_state`
- fixed `priority`
- optional dynamic `relevance`
- optional `validator`
- optional `completion`

Selection model:

1. reject invalid goals
2. ignore already-complete goals
3. compute `priority + dynamic relevance`
4. pick the highest score
5. if `preempt_on_better_goal` is enabled, switch only when the new goal beats the current one by `goal_switch_margin`

## Actions

Actions are symbolic planning units, not full gameplay stacks.

Each `ActionDefinition` contains:

- symbolic `preconditions`
- symbolic `effects`
- `base_cost`
- optional `dynamic_cost`
- optional `context_validator`
- optional target specification
- stable `executor` key
- `interruptible` flag (default `true`)

The planner reasons about the metadata above. The game owns the real execution that satisfies the action.

## Target-Aware Actions

Target-aware actions use:

- `ActionTargetSpec`
  symbolic slot name plus provider key
- `TargetProviderContext`
  entity, domain, goal, current symbolic state, and action definition
- `TargetCandidate`
  stable token, label, optional debug position, and optional cost bias

Planning flow:

1. target provider returns candidates
2. each candidate becomes a `PreparedActionVariant`
3. optional context validation filters candidates
4. optional dynamic cost hook adjusts each candidate
5. the cheapest valid target-bound plan wins

This is what lets one symbolic action represent "use workstation A" and "use workstation B" without making the planner blind to which target it actually chose.

## Sensors

Sensors are the boundary between rich ECS data and symbolic memory.

### Local sensors

- run per agent
- update agent-specific facts such as inventory, threat, or local cooldowns

### Global sensors

- run per domain
- update shared facts or expensive cached data once for many agents

Both sensor kinds:

- support polling intervals
- support explicit invalidation via messages
- update runtime bookkeeping so BRP and overlays can inspect last run time, next due time, and notes

## Plan Search

The planner performs forward A* over symbolic states.

Heuristic (h_max):

- for each unsatisfied goal condition, look up the minimum cost among actions whose effects can satisfy it (precomputed in `ActionRelevanceMap`)
- return the maximum of those per-condition costs
- admissible: never overestimates, since even an action satisfying multiple conditions still costs at least `max(min_cost_i)`

Tie-breaking:

- lower total estimated cost first
- then lower path cost
- then shallower depth
- then action declaration order / target candidate order
- then insertion order

That ordering keeps results deterministic for the same inputs.

## Replanning Policy

The runtime distinguishes several invalidation paths:

- `RequiredFactChanged`
  remaining step precondition no longer matches
- `TargetInvalidated`
  chosen target vanished or no longer passes context validation
- `ActionFailed`
  execution reported failure
- `HigherPriorityGoal`
  goal selection chose a better goal
- `SensorRefresh`
  a newer sensor revision made the current plan or in-flight planning session stale and the agent is configured to react to that
- `GoalNoLongerValid`
  goal validator failed
- `GoalCompleted`
  goal finished
- `Manual`
  external system explicitly invalidated the planner

Default execution policy:

- keep the current action alive while it is `Running` or `Waiting`
- invalidate immediately if the action's current target disappears
- invalidate immediately if the current step's required facts stop matching
- between actions, optionally invalidate on `SensorRefresh`
- after `PlanFailed`, hold that failure until the goal changes or sensors advance to a newer revision, instead of retrying the same impossible search every frame

## Execution Handoff

Execution is message-driven.

Planner to game:

- `ActionDispatched`

Game to planner:

- `ActionExecutionReport::Running`
- `ActionExecutionReport::Waiting`
- `ActionExecutionReport::Success`
- `ActionExecutionReport::Failure`
- `ActionExecutionReport::Cancelled`

That boundary keeps the crate generic. A game can resolve an action through movement code, animation code, network RPCs, or a custom worker system without changing the planner.

## World-State Hashing

`GoapWorldState` maintains an incrementally-updated Zobrist hash. Each `set_raw`, `clear`, `ensure_len`, and `overlay` mutation XORs out the old slot hash and XORs in the new one. The `Hash` trait uses the cached `u64` directly, and `PartialEq` fast-rejects on hash mismatch before comparing elements. This reduces per-node HashMap operations in the planner from O(keys) to O(1) average.

## Target Reservations in Planning

When `GoapDomainDefinition.reservation_policy` is set, the planning problem construction phase checks each target candidate against `GoapReservationMap`. Targets reserved by other agents receive `cost_penalty` additional cost (or are skipped with `hard_block: true`). This steers the planner toward unreserved targets without hard-gating the ECS. Reservations are acquired on plan acceptance and released on plan invalidation or completion.
