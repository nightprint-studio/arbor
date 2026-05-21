
-- ── Recipe: ad-hoc modal form ────────────────────────────────────────────────
-- arbor.ui.show_form opens a one-shot modal; the submit callback receives the
-- typed values as a Lua table.
local function open_demo_modal()
  arbor.ui.show_form({
    title  = "Quick input",
    fields = {
      { id = "branch", type = "text",   label = "Branch name", required = true },
      { id = "draft",  type = "toggle", label = "Open as draft" },
    },
    on_submit = function(values)
      arbor.notify{ message = "Got: " .. (values.branch or ""), level = "info" }
    end,
  })
end
