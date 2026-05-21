
-- ── Recipe: toolbar action on the graph header ───────────────────────────────
-- Adds a button to the graph action bar (next to Pull / Fetch / …). Clicking
-- it fires `__SLUG__:toolbar` which is handled below.
arbor.ui.add_graph_action({
  id      = "__SLUG__-toolbar",
  icon    = "Sparkles",
  label   = "__SLUG__",
  tooltip = "Run the main __SLUG__ action",
  action  = "__SLUG__:toolbar",
})

arbor.events.on("__SLUG__:toolbar", function(_ctx)
  arbor.notify{ message = "Toolbar action fired", level = "info" }
end)
