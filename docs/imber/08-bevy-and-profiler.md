---
title: "Imber: Bevy games and the profiler"
summary: "Looking into a running Bevy game over BRP, changing it behind a separate switch, and reading a profiler recording."
keywords: ["bevy", "brp", "remote protocol", "profiler", "recording", "entities", "components"]
related: ["Imber: Tools it can use", "Imber: Permissions and modes"]
source: src/lib/components/bennu/docs/imber/Bevy.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Bevy games and the profiler

Imber can look into a running Bevy game over its Remote Protocol, change it only if you allow that, and read the newest profiler recording to propose a performance change.

## Looking into a running game

The *Look into a running Bevy game* family (Settings › Imber › Tools it may use, for projects with a `Cargo.toml`) connects to a game over the Bevy Remote Protocol. The game needs `RemotePlugin` and `RemoteHttpPlugin`, and the connection is to this machine only.

- `brp_connect`: connects, and waits for the game to come up if it is still starting.
- `brp_query`: entities with their component values.
- `brp_get`: one entity in full.
- `brp_resources` and `brp_get_resource`: the resources and one resource's value.
- `brp_types`: the type registry, so it knows the shape of a component before it asks for it.

Reading never asks. The connection itself and the protocol are explained on the **Bevy Remote** page.

## Changing a running game

*Let it change a running Bevy game* in Settings is a separate switch, off by default, and it needs the Bevy family on. It adds `brp_mutate`, `brp_insert`, `brp_remove`, `brp_spawn`, `brp_despawn` and `brp_set_resource`. These act on live state with no review and no undo. Imber asks once per conversation ("Let Imber change the running game?"), the yes covers every change in that conversation, and every change is listed in the run's summary. Autonomous mode does not skip the question.

## Reading a profiler recording

The project-intelligence family includes `bennu_profile_summary`: a compact summary of the newest recording the Profiler tool window made (the hottest methods with their source lines, the biggest allocation sites and allocated types, lock contention and garbage-collection pauses). Imber uses it to find the method worth optimising and to propose a change. It only reads: it never starts a recording, and with no recording it tells you to make one first. Recording is described on the **Profiler** page.
