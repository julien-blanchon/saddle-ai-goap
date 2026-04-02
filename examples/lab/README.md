# GOAP Lab

Crate-local standalone lab app for validating the shared `saddle-ai-goap` crate in a live Bevy scene.

## Purpose

- verify target-aware replanning when a chosen target disappears
- verify multi-step worker planning with local and global sensors
- expose stable named entities, overlay diagnostics, BRP resources, and screenshot hooks
- keep richer verification inside the shared crate instead of a project-level sandbox

## Status

Working

## Run

```bash
cargo run -p saddle-ai-goap-lab
```

## E2E

```bash
cargo run -p saddle-ai-goap-lab --features e2e -- smoke_launch
cargo run -p saddle-ai-goap-lab --features e2e -- goap_smoke
cargo run -p saddle-ai-goap-lab --features e2e -- goap_replan
cargo run -p saddle-ai-goap-lab --features e2e -- goap_worker_cycle
```

## BRP

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

## Notes

- The scene keeps one guard lane and one worker lane alive at the same time so different GOAP use cases share the same runtime surface.
- The overlay mirrors `GoapDebugSnapshot` and selected diagnostics so screenshot checkpoints stay readable.
- The lab intentionally uses simple generated geometry instead of external assets because the planner behavior, not art content, is the verification target here.
