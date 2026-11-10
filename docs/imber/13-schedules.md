---
title: "Imber: Schedules"
summary: "Ask Imber something on a timetable: how to create a schedule, what a scheduled run may do, missed runs, cost limits and the MCP tools."
keywords: ["schedule", "cron", "recurring", "timetable", "every day", "weekday", "run now", "missed", "read-only", "mcp", "bennu_imber_schedules", "bennu_imber_schedule_create"]
related: ["Imber: Cost and context", "Imber: Permissions and modes", "Imber: Settings reference"]
source: src/lib/components/bennu/docs/imber/Schedules.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Schedules

A schedule asks Imber something at a time you choose: once, every N minutes, every day, on chosen weekdays, or on a cron expression. "Every weekday at 09:00, tell me what changed on main" is one schedule.

## Creating one

- **By hand.** Open the window from **Schedules…** in the panel's **…** menu or the palette entry *Imber: Schedules…*, then **New** (or press `N` in the list).
- **From the composer.** Type the prompt, then run *Imber: Schedule this prompt…* from the palette: the form opens with the text already in it. The entry appears only when the composer has text and a project is open.
- **By asking Imber.** Say it in a conversation: "every weekday at 9, check what changed on main". Imber always shows the schedule in words and waits for your approval before it exists, whatever the access mode.

The form is short: name, when, what it does, what to do when the run was missed, and an on/off switch. The name takes the focus when it opens, `Tab` goes through the fields, `Ctrl+Enter` saves and `Esc` goes back.

## What a schedule does

| Action | What happens |
| --- | --- |
| **Wake this conversation** | The saved prompt is sent into the conversation the schedule was made from, which carries on as if you had written it. |
| **Ask Imber in a new conversation** | A fresh conversation each time, titled after the schedule. The outcome is shown on the schedule. |

Write the prompt so that it needs no answer: say what to check and what to report. Nobody is there to reply.

## A scheduled run is read-only

- The run reads and searches the project, and any change it wants to make is left as a proposal in its conversation, waiting for you. It never modifies the project, runs commands or asks for confirmation.
- If Imber asks a question, the run ends as *incomplete* instead of waiting.
- A scheduled run cannot create, change or delete schedules.
- A schedule Imber creates is no freer than the conversation that created it, and an MCP client can only create read-only ones.

## Only while Bennu is open

Schedules fire only while Bennu is running; there is no background service. When a run's time passed while Bennu was closed, the schedule decides: **Skip it** (the default) drops it, and the schedule shows it as *Skipped* rather than as a failure; **Run it once on the next start** catches up with a single run. A one-time schedule that was missed and skipped turns itself off.

## Cost limits

Scheduled runs spend from the same limits as the panel and are also capped on their own: a **scheduled run cap** (euros per run; the run ends as incomplete when it gets there) and a number of **scheduled runs per day**, so a mistaken schedule cannot spend the month overnight. Both are in Settings › Imber › Spending limits; 0 switches one off.

## Knowing it ran

A small toast says "Schedule … started" at every start, including **Run now**, and a failure shows its reason, once per episode rather than at every attempt. The list shows when each schedule runs next and how the last run ended.

## The window

| Key | Action |
| --- | --- |
| `↑` `↓` | Move between schedules |
| `Enter` | Edit the selected schedule |
| `Space` | Turn it on or off |
| `R` | Run it now, outside its times (its next run is unchanged) |
| `Del` | Delete it, after a confirmation |
| `N` | New schedule |

A schedule that wakes a conversation also has a button that opens it.

## From other tools

AI clients connected over MCP get two tools: `bennu_imber_schedules` lists the schedules, and `bennu_imber_schedule_create` creates a read-only one. Neither can change or delete an existing schedule.

## Where they are saved

The list lives in Bennu's config folder, outside your projects, so a run's bookkeeping never shows up in version control. Each schedule belongs to the project that was open when it was made.
