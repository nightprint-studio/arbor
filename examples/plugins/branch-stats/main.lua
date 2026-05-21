-- branch-stats / main.lua
-- Minimal example: logs branch lifecycle events and demonstrates the UI API.

-- ── Lifecycle ─────────────────────────────────────────────────────────────────

arbor.events.on("on_plugin_load", function(ctx)
  arbor.log.info("ready (api_version=" .. ctx.api_version .. ")")
end)

-- ── UI registrations ──────────────────────────────────────────────────────────

arbor.ui.add_context_menu_item({
  target = "commit",
  label  = "Inspect Commit",
  action = "inspect:inspect_commit",
  icon   = "Search",
})

arbor.ui.add_menu_item({
  label  = "Branch Stats: Run Report",
  action = "inspect:run_report",
})

arbor.ui.add_sidebar({
  id      = "branch-stats",
  label   = "Branch Stats",
  icon    = "BarChart2",
  side    = "right",
  tooltip = "Branch Stats",
})

-- Plugin panel populated on demand: when the user clicks the icon Arbor
-- fires `panel:open:branch-stats` and we respond with set_panel_content.
arbor.events.on("panel:open:branch-stats", function(_ctx)
  arbor.ui.set_panel_content("branch-stats", {
    title = "Branch Stats",
    nodes = {
      { type = "paragraph",
        text = "Open the report from the menu, or run from the hamburger menu." },
      { type = "button", label = "Run Report", icon = "Play",
        action = "inspect:run_report" },
    },
  })
end)

-- ── Hooks ─────────────────────────────────────────────────────────────────────

arbor.events.on("on_commit", function(ctx)
  arbor.log.info("on_commit oid=" .. (ctx.oid or "?") .. " branch=" .. (ctx.branch or "?"))
  arbor.notify{ message = "Commit recorded by Branch Stats", level = "success" }
end)

arbor.events.on("on_checkout", function(ctx)
  arbor.log.info("on_checkout branch=" .. (ctx.branch or "?") ..
    " (was " .. (ctx.previous_branch or "?") .. ")")
end)

arbor.events.on("on_fetch", function(ctx)
  arbor.notify{ message = "Fetch complete — Branch Stats updated", level = "info" }
end)

-- ── Action handlers ───────────────────────────────────────────────────────────

arbor.events.on("inspect:inspect_commit", function(ctx)
  arbor.log.info("inspect_commit oid=" .. (ctx.oid or "?"))
  arbor.ui.form()
    :title("Inspect Commit")
    :description("Add a personal note for this commit.")
    :state({ oid = ctx.oid })
    :textarea("note", { label = "Note",
      placeholder = "What's interesting about this commit?", rows = 3 })
    :text("tag", { label = "Tag",
      placeholder = "e.g. performance, fix, refactor" })
    :checkbox("bookmark", { label = "Bookmark this commit" })
    :submit("Save Note", "inspect:save_note")
    :on_cancel("inspect:cancel_note")
    :open()
end)

arbor.events.on("inspect:save_note", function(ctx)
  arbor.log.info("note saved for oid=" .. (ctx.state and ctx.state.oid or "?") ..
    " tag=" .. (ctx.tag or ""))
  arbor.notify{ message = "Note saved successfully!", level = "success" }
end)

arbor.events.on("inspect:cancel_note", function(_ctx)
  -- User cancelled — nothing to do.
end)

arbor.events.on("inspect:run_report", function(_ctx)
  arbor.notify{ message = "Branch Stats report generated (see log)", level = "info" }
  arbor.log.info("running branch stats report…")
end)
