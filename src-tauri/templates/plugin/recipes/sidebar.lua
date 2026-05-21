
-- ── Recipe: right-side panel ─────────────────────────────────────────────────
-- Lazy-rendered: arbor.ui.set_panel_content is called every time the user
-- opens the panel so plugins can refresh content on demand.
arbor.ui.add_sidebar({
  id       = "overview",
  icon     = "Puzzle",
  label    = "__SLUG__",
  tooltip  = "__SLUG__ overview panel",
  side     = "right",
  position = "top",
})

arbor.events.on("panel:open:overview", function(_ctx)
  arbor.ui.set_panel_content("overview", {
    title = "__SLUG__",
    nodes = {
      { type = "heading", text = "Hello from __SLUG__" },
      { type = "label",   text = "Edit main.lua to customise this panel." },
      { type = "divider" },
      { type = "button",  label = "Click me", action = "__SLUG__:run" },
    },
  })
end)
