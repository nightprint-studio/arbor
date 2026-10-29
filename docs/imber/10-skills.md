---
title: "Imber: Skills"
summary: "SKILL.md folders that teach Imber a kind of task: format, locations, the manager, limits."
keywords: ["skill", "SKILL.md", "slash", "load_skill", "claude skills", "manager"]
related: ["Imber: Settings reference", "Imber: Privacy and safety"]
source: src/lib/components/bennu/docs/imber/Skills.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Skills

A skill is a folder with a `SKILL.md` that teaches Imber how you want one kind of task done; none is known to Imber until you enable it in the skills manager.

## The format

The format is the one Claude Code uses: a `SKILL.md` with a frontmatter holding `name` and `description`, then the instructions. Examples: a release checklist, how to write a migration, how your team names tests.

```
---
name: write-migration
description: How to write a database migration in this project
---
1. Put the file in db/migrations, named V<next number>__<what>.sql.
2. Never edit a migration that has been applied.
```

## Where skills are looked for

- each project's `.bennu/skills` and `.claude/skills`;
- Bennu's own `skills` folder in its config folder;
- `~/.claude/skills`.

A project skill hides a user skill of the same name, and `.bennu` wins over `.claude`.

## The skills manager

Open it from **Skills…** in the panel's **…** menu, the composer's settings menu, Settings › [Imber › Skills](10-skills.md), or the palette entry *Imber: Manage skills…*. It lists each skill with where it came from, its size and a preview of its text; switches them one by one or all at once; and lists folders it could not use with the reason. The Settings page and the composer show "N of M enabled".

Enabling is an allow-list stored in the config (`skills_on`, ids of the form `user/name` or `project/name`): a skill found on disk but not ticked is invisible to Imber.

## How a skill is used

- Only a one-line index of the enabled skills (name and description) travels with each request.
- Imber loads a skill's text with `load_skill` when a task matches, or when you start your message with `/name`. Type `/` in the composer to pick from the enabled skills.
- Limits: a skill's text is cut at 32 KB, at most 50 skills are read, and a description is cut at 240 characters.
- Nothing in a skill is ever run. Other files in its folder are only named; Imber can read them with its ordinary tools only if the reach allows it.

Skills are not available in a chat.
