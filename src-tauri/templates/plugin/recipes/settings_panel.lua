
-- ── Recipe: settings panel (gear icon in Plugin Manager) ─────────────────────
-- arbor.ui.container.register exposes a form-driven settings panel. Values are
-- persisted automatically via arbor.settings.global.* and surfaced again on
-- next form open.
arbor.ui.container.register({
  id     = "settings",
  title  = "__SLUG__ settings",
  layout = "form",
  width  = 520,
  fields = {
    { id = "enabled",   type = "toggle", label = "Enable feature", default = true },
    { id = "username",  type = "text",   label = "Username", placeholder = "octocat" },
    { id = "max_items", type = "number", label = "Max items", default = 50, min = 1, max = 999 },
  },
  on_submit = function(values)
    for k, v in pairs(values) do arbor.settings.global.set(k, v) end
    arbor.notify{ message = "Settings saved", level = "success" }
  end,
})
