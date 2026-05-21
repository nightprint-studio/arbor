
-- ── Recipe: command palette entry ────────────────────────────────────────────
-- Registered commands appear in the Command Palette (Ctrl+K). Selecting one
-- fires `command:<id>` on this plugin — handled by the events.on below.
arbor.command.register({
  id          = "say-hello",
  title       = "__SLUG__: Say Hello",
  description = "A friendly greeting from the __SLUG__ plugin",
  icon        = "MessageSquare",
})

arbor.events.on("command:say-hello", function(_ctx)
  arbor.notify{ title = "__SLUG__", message = "Hello from your new plugin!", level = "info" }
end)
