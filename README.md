# Saddle AI GOAP

Reusable Goal-Oriented Action Planning runtime for Bevy.

The crate keeps planning definitions shared at the app level and per-agent runtime state on entities. It is intentionally project-agnostic: it does not assume a specific combat stack, navigation system, inventory model, animation graph, or state machine. Game code provides sensors, target providers, dynamic goal scoring, action validation, and action execution.

For apps where planners should stay live for the full app lifetime, prefer `GoapPlugin::always_on(Update)`. Use `GoapPlugin::new(...)` when activation should follow explicit schedules such as `OnEnter` / `OnExit`.

## Quick Start

```toml
[dependencies]
saddle-ai-goap = { git = "https://github.com/julien-blanchon/saddle-ai-goap" }
```

```rust,no_run
use bevy::prelude::*;
use saddle_ai_goap::{
    ActionDefinition, ActionDispatched, ActionExecutionReport, ActionExecutionStatus, ActionId,
    GoalDefinition, GoalId, GoapAgent, GoapLibrary, GoapPlugin,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(GoapPlugin::always_on(Update))
        .add_systems(Startup, setup)
        .add_systems(Update, finish_actions)
        .run();
}

fn setup(mut commands: Commands, mut library: ResMut<GoapLibrary>) {
    let mut domain = saddle_ai_goap::GoapDomainDefinition::new("basic");
    let done = domain.add_bool_key("done", Some("work finished".into()), Some(false));
    domain.add_goal(
        GoalDefinition::new(GoalId(0), "finish work")
            .with_priority(10)
            .with_desired_state([saddle_ai_goap::FactCondition::equals_bool(done, true)]),
    );
    domain.add_action(
        ActionDefinition::new(ActionId(0), "finish work", "finish_work")
            .with_effects([saddle_ai_goap::FactEffect::set_bool(done, true)]),
    );

    let domain_id = library.register(domain);

    commands.spawn((
        Name::new("Worker"),
        GoapAgent::new(domain_id),
    ));
}

fn finish_actions(
    mut dispatched: MessageReader<ActionDispatched>,
    mut reports: MessageWriter<ActionExecutionReport>,
) {
    for message in dispatched.read() {
        if message.executor.as_str() == "finish_work" {
            reports.write(ActionExecutionReport::new(
                message.entity,
                message.ticket,
                ActionExecutionStatus::Success,
            ));
        }
    }
}
```

## Public API

- Plugin: `GoapPlugin`
- System sets: `GoapSystems::{Sense, SelectGoal, Plan, Dispatch, Monitor, Cleanup, Debug}`
- Components: `GoapAgent`, `GoapRuntime`, `GoapPlan`, `ActiveAction`, `GoapDebugSnapshot`
- Resources: `GoapLibrary`, `GoapHooks`, `GoapPlannerScheduler`, `GoapGlobalSensorCache`
- Definition types: `GoapDomainDefinition`, `GoalDefinition`, `ActionDefinition`, `SensorDefinition`
- Planner types: `PlanningProblem`, `PlanningSession`, `GoapPlannerLimits`, `SelectedGoal`, `TargetCandidate`
- World-state types: `WorldStateSchema`, `WorldKeyId`, `FactValue`, `FactCondition`, `FactEffect`, `FactPatch`, `TargetToken`
- Messages: `GoalChanged`, `PlanStarted`, `PlanCompleted`, `PlanFailed`, `PlanInvalidated`, `ActionDispatched`, `ActionCancelled`, `ActionExecutionReport`, `InvalidateGoapAgent`, `InvalidateLocalSensors`, `InvalidateGlobalSensors`

## Core Model

- Shared definitions, per-agent runtime:
  `GoapLibrary` stores reusable immutable-ish domain definitions; entities store only runtime state, the current goal, the active plan cursor, active action tracking, counters, and sensor timing.
- Agent-centric symbolic memory:
  planning reads `GoapWorldState`, not broad ECS queries. Sensors curate the symbolic state from world data.
- Target-aware actions:
  target providers generate `TargetCandidate` values per action slot, and planners evaluate each candidate as its own symbolic action variant.
- Interruptible execution:
  planning dispatches an `ActionDispatched` message and waits for `ActionExecutionReport` messages with `Running`, `Waiting`, `Success`, `Failure`, or `Cancelled`.
- Budgeted planning:
  `PlanningSession` supports incremental A* search, and `GoapPlannerScheduler` limits how many agents advance their planning work per frame.
- Layered planner budgets:
  `GoapDomainDefinition::with_default_limits(...)` sets domain-wide defaults, while `GoapAgentConfig::with_planner_limits(...)` lets specific agents override them.
- Failed-plan retry gating:
  when a goal fails at a specific sensor revision, the runtime will not spam identical replans every frame; it waits for a goal change, invalidation, or newer sensed state before retrying.

## Replanning Policy

The runtime exposes deliberate replan triggers instead of hiding replanning inside ad-hoc execution code:

- `RequiredFactChanged`:
  the current step's symbolic preconditions no longer match sensed state.
- `TargetInvalidated`:
  the current step's chosen target no longer exists or fails context validation.
- `ActionFailed`:
  game-side execution reported failure.
- `HigherPriorityGoal`:
  goal selection found a more relevant goal than the active one.
- `SensorRefresh`:
  enabled by `GoapAgentConfig::replan_on_sensed_state_change`; when no action is currently running, a sensor revision newer than the current plan invalidates stale assumptions and queues a rebuild.
- `GoalCompleted` / `GoalNoLongerValid`:
  the active goal is done or no longer passes its validator.

The default policy is conservative while an action is running: the crate keeps the action alive until it fails, succeeds, loses a required target, or loses a currently-required fact. Once the agent is between actions, a newer sensor revision can invalidate the remainder of the plan.

## Examples

| Example | Description | Run |
| --- | --- | --- |
| `basic` | Minimal single-agent plan with one action and one completion report | `cargo run -p saddle-ai-goap --example basic` |
| `guard_replan` | Target-aware guard behavior where the first target disappears and the agent replans | `cargo run -p saddle-ai-goap --example guard_replan` |
| `worker_cycle` | Multi-step economy loop with local and global sensors | `cargo run -p saddle-ai-goap --example worker_cycle` |
| `saddle-ai-goap-lab` | Crate-local showcase app with BRP and E2E hooks | `cargo run -p saddle-ai-goap-lab` |

## Crate-Local Lab

`shared/ai/saddle-ai-goap/examples/lab` is the richer verification surface for this crate. It keeps target loss, worker replanning, overlay diagnostics, BRP resources, and E2E scenarios inside the shared crate instead of pushing them into project-level sandboxes.

```bash
cargo run -p saddle-ai-goap-lab
```

E2E commands:

```bash
cargo run -p saddle-ai-goap-lab --features e2e -- smoke_launch
cargo run -p saddle-ai-goap-lab --features e2e -- goap_smoke
cargo run -p saddle-ai-goap-lab --features e2e -- goap_replan
cargo run -p saddle-ai-goap-lab --features e2e -- goap_worker_cycle
```

## BRP

Useful BRP commands against the lab:

```bash
uv run --active --project .codex/skills/bevy-brp/script brp app launch saddle-ai-goap-lab
uv run --active --project .codex/skills/bevy-brp/script brp world query bevy_ecs::name::Name
uv run --active --project .codex/skills/bevy-brp/script brp world query saddle_ai_goap::components::GoapAgent
uv run --active --project .codex/skills/bevy-brp/script brp world query saddle_ai_goap::components::GoapRuntime
uv run --active --project .codex/skills/bevy-brp/script brp world query saddle_ai_goap::debug::GoapDebugSnapshot
uv run --active --project .codex/skills/bevy-brp/script brp resource get saddle_ai_goap::resources::GoapPlannerScheduler
uv run --active --project .codex/skills/bevy-brp/script brp resource get saddle_ai_goap::resources::GoapGlobalSensorCache
uv run --active --project .codex/skills/bevy-brp/script brp extras screenshot /tmp/saddle_ai_goap_lab.png
uv run --active --project .codex/skills/bevy-brp/script brp extras shutdown
```

## Limitations

- The crate keeps the symbolic state intentionally compact: booleans, integers, and target tokens are first-class. Rich spatial reasoning should usually stay in target providers and context validators instead of being pushed into float-heavy symbolic state.
- Reservations, plan caching, and squad-level coordination are not built in.
- The runtime does not ship a genre-specific action executor. Games are expected to own the actual locomotion, animation, combat, or crafting behavior that satisfies dispatched actions.

## More Docs

- [Architecture](docs/architecture.md)
- [Configuration](docs/configuration.md)
- [Planning Model](docs/planning-model.md)
- [Debugging](docs/debugging.md)
