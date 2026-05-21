
-- ── Recipe: background job ───────────────────────────────────────────────────
-- arbor.job.spawn streams stdout/stderr to the Jobs panel and returns a
-- handle the user can cancel. Requires `terminal` permission.
local function run_build()
  return arbor.job.spawn({
    name    = "__SLUG__ build",
    command = "echo",
    args    = { "hello", "from", "__SLUG__" },
    on_done = function(result)
      local lvl = result.success and "success" or "error"
      arbor.notify{ message = "__SLUG__: build " .. (result.success and "ok" or "failed"), level = lvl }
    end,
  })
end
