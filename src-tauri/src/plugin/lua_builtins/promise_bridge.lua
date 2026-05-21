-- Promise bridge — wraps the Rust-backed async APIs so they return arbor.async.Promise.
-- Loaded by sandbox.rs *after* arbor.* is published. Idempotent: if a subsystem
-- isn't present (permission gated) we skip its wrapper.
--
-- Conventions:
--   · service.call(qualified, args [, cb])   → Promise; cb is sugar — it still
--     fires with `(ok, value_or_err)` so older callers keep working.
--   · job.spawn(config)                      → (JobHandle, err) where JobHandle
--     is a Promise with `.id` and `:cancel()`. Resolves with the on_done ctx
--     when the job exits successfully, rejects with the same shape on failure.
--     `config.on_done` is sugar — it still fires alongside the promise.
--   · ui.confirm{message, …}                 → Promise resolving to `true`
--     (confirmed) or `false` (cancelled). No more confirm_action / cancel_action.

local async = require("arbor.async")
local Promise = async.Promise

-- ─── arbor.service.call wrapper ────────────────────────────────────────────
if arbor.service and arbor.service.call then
    local raw_call = arbor.service.call
    arbor.service.call = function(qualified, args, cb)
        local p = Promise.new()
        raw_call(qualified, args, function(ok, value)
            if ok then p:_resolve(value) else p:_reject(value) end
            if cb then
                local ok2, err2 = pcall(cb, ok, value)
                if not ok2 then arbor.log.error("service.call cb error: " .. tostring(err2)) end
            end
        end)
        return p
    end
end

-- ─── arbor.job.spawn wrapper ───────────────────────────────────────────────
if arbor.job and arbor.job.spawn then
    local raw_spawn  = arbor.job.spawn
    local raw_cancel = arbor.job.cancel

    arbor.job.spawn = function(config)
        local p = Promise.new()
        local user_on_done = config.on_done
        config.on_done = function(ctx)
            if ctx and ctx.success then
                p:_resolve(ctx)
            else
                p:_reject(ctx)
            end
            if user_on_done then
                local ok, err = pcall(user_on_done, ctx)
                if not ok then arbor.log.error("job.spawn on_done error: " .. tostring(err)) end
            end
        end

        local id, err = raw_spawn(config)
        if not id then
            -- Spawn refused (lock error, missing handle): surface as tuple error.
            return nil, err
        end

        p.id = id
        function p:cancel() raw_cancel(self.id) end
        return p, nil
    end
end

-- ─── arbor.ui.confirm wrapper ──────────────────────────────────────────────
if arbor.ui and arbor.ui.confirm then
    local raw_confirm = arbor.ui.confirm
    local counter = 0

    arbor.ui.confirm = function(opts)
        if type(opts) ~= "table" then
            error("arbor.ui.confirm: expected a table { message = … }", 2)
        end
        local msg = opts.message
        if type(msg) ~= "string" or msg == "" then
            error("arbor.ui.confirm: 'message' is required (string)", 2)
        end

        counter = counter + 1
        local id             = counter
        local confirm_action = "__arbor_confirm_" .. id .. "_ok__"
        local cancel_action  = "__arbor_confirm_" .. id .. "_cancel__"

        local p = Promise.new()
        local hooks = _G.__arbor_hooks__
        local function cleanup()
            if hooks then
                hooks[confirm_action] = nil
                hooks[cancel_action]  = nil
            end
        end
        local function once(action, value)
            arbor.events.on(action, function()
                if p:is_pending() then
                    cleanup()
                    p:_resolve(value)
                end
            end)
        end
        once(confirm_action, true)
        once(cancel_action,  false)

        raw_confirm(msg, {
            confirm_label   = opts.confirm_label,
            confirm_variant = opts.confirm_variant,
            confirm_action  = confirm_action,
            cancel_action   = cancel_action,
            state           = opts.state,
        })
        return p
    end
end
