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

The project-intelligence family includes three read-only profiler tools. `bennu_profile_recordings` lists the recordings the Profiler tool window made. `bennu_profile_diff` compares two of them method by method (the share of the run each took before and after, biggest changes first), to check that a change made the code faster. `bennu_profile_summary` is a compact summary of one recording (the newest unless Imber names another): the hottest methods with their source lines, the biggest allocation sites and allocated types, lock contention and garbage-collection pauses. Imber uses it to find the method worth optimising and to propose a change. These three only read: with no recording they tell you to make one first. Recording by hand is described on the **Profiler** page; Imber recording for you is below.

## Recording a running JVM

The **Profile** family (`profile_start`, `profile_stop`) lets Imber record a JVM that is already running on this machine with Flight Recorder, through `jcmd`. **Imber asks at every start**, naming the JVM, because a recording samples a running program (nothing in it is changed, it runs a little slower). Imber names the JVM by process id or by part of its main class; with neither, it gets the list to choose from. One recording per conversation, started in one message and stopped in a later one. `profile_stop` ends the recording, waits until it has been read and saved, and returns its summary; the recording is then in the Profiler window like any other. A recording keeps sampling until it is stopped. Sub-agents never have these tools, and the family can be switched off like any other.
