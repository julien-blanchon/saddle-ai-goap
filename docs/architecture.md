# Architecture

`saddle-ai-goap` splits the runtime into three layers:

1. Shared domain definitions in `GoapLibrary`
2. Per-agent runtime state on `GoapRuntime`
3. Game-owned hooks for sensing, scoring, target selection, validation, and execution

That split keeps planning data reusable while avoiding deep per-entity definition clones.

## Why Forward A*

The planner uses forward A* search over the agent's current symbolic state.

This choice is deliberate:

- action effects are already authored as forward symbolic mutations
- target-aware action variants are easy to enumerate as concrete successor nodes
- dynamic cost hooks fit naturally into successor expansion
- incremental search across frames is straightforward because the open set stores future states directly

Tradeoffs:

- regressive planning can reason from the goal backwards with smaller branching in some domains
- forward planning can expand more nodes when many actions are available early
- target-heavy domains can multiply the branching factor if target providers return too many candidates

The crate addresses those tradeoffs with explicit budgets:

- `max_node_expansions`
- `max_plan_length`
- `max_expansions_per_step`
- `GoapPlannerScheduler::max_agents_per_frame`

## Data Flow

```text
ECS world
  -> local/global sensors
  -> GoapWorldState
  -> goal selection
  -> planning problem
  -> incremental A* search
  -> GoapPlan
  -> ActionDispatched
  -> game-side execution
  -> ActionExecutionReport
  -> plan monitoring / invalidation / completion
```

## Shared vs Per-Agent Storage

Shared:

- `GoapLibrary`
  domain schemas, goals, actions, and sensor definitions
- `GoapHooks`
  app-level sensor, scoring, validation, target, and dynamic-cost handlers
- `GoapGlobalSensorCache`
  domain-scoped symbolic cache for expensive shared sensor work
- `GoapPlannerScheduler`
  fairness queue and per-frame planner budget

Per agent:

- `GoapAgent`
  domain binding and per-agent config
- `GoapRuntime`
  sensed state, active goal, plan cursor, active action, counters, sensor timing, failed-plan retry bookkeeping, and optional incremental planning session
- `GoapDebugSnapshot`
  BRP-friendly current goal, plan chain, targets, invalidation reason, and counter summary

Asset-authored domains load through `GoapDomainAssetLoader`, then register into `GoapLibrary` exactly like code-built domains. The planner therefore sees one normalized source of truth after load time.

## Runtime Pipeline

The public runtime phases are:

```text
Sense -> SelectGoal -> Plan -> Dispatch -> Monitor -> Cleanup -> Debug
```

### `GoapSystems::Sense`

- initialize late-spawned agents
- refresh global sensors on interval or invalidation
- refresh local sensors on interval or invalidation
- update sensed symbolic state and sensor revisions

### `GoapSystems::SelectGoal`

- score and validate goals
- preempt current goals when a better one becomes relevant
- invalidate stale plans when sensor revisions moved past the current plan and the agent is configured to replan on sensor refresh
- queue planning work

### `GoapSystems::Plan`

- dequeue up to `max_agents_per_frame` agents
- build a `PlanningProblem`
- advance an incremental `PlanningSession`
- publish `PlanStarted` or `PlanFailed`
- remember the sensor revision of failed plans so identical retries stay blocked until the world state or goal changes

### `GoapSystems::Dispatch`

- convert the current step into `ActionDispatched`
- create a stable ticket for execution feedback
- move the agent into `WaitingOnAction`

### `GoapSystems::Monitor`

- consume `ActionExecutionReport`
- invalidate plans on failed preconditions, target loss, explicit invalidations, or sensor-refresh policy
- complete goals when their desired conditions or completion hook says they are done

### `GoapSystems::Cleanup`

- remove runtime state from entities that lost `GoapAgent`

### `GoapSystems::Debug`

- write a compact `GoapDebugSnapshot` for overlay UI, BRP, and inspection tools

## Target-Aware Planning

Actions with `ActionTargetSpec` do not plan against a generic anonymous target. Instead:

1. a target provider returns candidate `TargetCandidate` values
2. each candidate becomes a concrete `PreparedActionVariant`
3. context validators and dynamic cost hooks evaluate the action with that target bound
4. the chosen target is preserved on the plan step and echoed in `ActionDispatched`

This keeps the planner symbolic while letting game code own the expensive spatial reasoning.

## Sensor Policy

Sensors are first-class because the planner should reason over curated memory, not raw ECS state.

- local sensors update agent-specific symbolic facts
- global sensors update shared domain caches
- both support interval polling plus explicit invalidation messages
- sensor refreshes increment a revision counter

That revision counter is what makes deliberate stale-plan invalidation possible. The planner can explain that a replan happened because symbolic memory changed, not because some hidden gameplay system silently discarded the plan.
