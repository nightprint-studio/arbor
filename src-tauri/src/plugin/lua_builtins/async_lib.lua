-- arbor.async — Promise primitive + debounce/throttle helpers
-- require("arbor.async")
--
-- Promise contract (Phase 4):
--   · `arbor.async.Promise.new()` → pending promise
--   · `p:ok(fn)` / `p:err(fn)` — attach success / failure listeners,
--     fire immediately if the promise is already settled. Returns `p`
--     so calls chain (`p:ok(...):err(...)`).
--   · `p:and_then(on_ok, on_err?)` — flat-map; returns a new promise that
--     adopts the value (or another promise) returned by the handler.
--   · `arbor.async.await(p)` — yields the current coroutine until `p`
--     settles. Returns `(value, nil)` on resolve, `(nil, err)` on reject.
--     Must be called inside a coroutine started by `arbor.async.run`.
--   · `arbor.async.run(fn, ...)` — spawn a coroutine that drives await;
--     any exception inside `fn` is logged via `arbor.log.error`.
--
-- Producers (`arbor.service.call`, `arbor.job.spawn`, `arbor.ui.confirm`)
-- return promises directly; consumers use `:ok / :err` or `await` — never
-- both `:_resolve` / `:_reject` (those are private to the producer).

local M = {}

-- ── Promise -----------------------------------------------------------------

local Promise = {}
Promise.__index = Promise
M.Promise = Promise

local function is_promise(v)
    return type(v) == "table" and getmetatable(v) == Promise
end
M.is_promise = is_promise

function Promise.new()
    return setmetatable({
        _state    = "pending",   -- "pending" | "fulfilled" | "rejected"
        _value    = nil,
        _waiters  = {},          -- list of { ok = fn?, err = fn? }
    }, Promise)
end

function Promise.resolved(value)
    local p = Promise.new()
    p._state = "fulfilled"
    p._value = value
    return p
end

function Promise.rejected(err)
    local p = Promise.new()
    p._state = "rejected"
    p._value = err
    return p
end

local function safe_call(fn, value)
    local ok, res = pcall(fn, value)
    if not ok then
        if arbor and arbor.log and arbor.log.error then
            arbor.log.error("promise handler error: " .. tostring(res))
        end
    end
    return ok, res
end

function Promise:_settle(state, value)
    if self._state ~= "pending" then return end
    -- Adopt nested promise: if the value is itself a promise, chain through it.
    if is_promise(value) and state == "fulfilled" then
        value:ok(function(v) self:_settle("fulfilled", v) end)
        value:err(function(e) self:_settle("rejected",  e) end)
        return
    end
    self._state = state
    self._value = value
    local waiters = self._waiters
    self._waiters = nil
    for _, w in ipairs(waiters) do
        if state == "fulfilled" and w.ok then safe_call(w.ok, value) end
        if state == "rejected"  and w.err then safe_call(w.err, value) end
    end
end

function Promise:_resolve(value) self:_settle("fulfilled", value) end
function Promise:_reject(err)    self:_settle("rejected",  err)   end

function Promise:ok(fn)
    if type(fn) ~= "function" then
        error("Promise:ok expects a function", 2)
    end
    if self._state == "fulfilled" then
        safe_call(fn, self._value)
    elseif self._state == "pending" then
        table.insert(self._waiters, { ok = fn })
    end
    return self
end

function Promise:err(fn)
    if type(fn) ~= "function" then
        error("Promise:err expects a function", 2)
    end
    if self._state == "rejected" then
        safe_call(fn, self._value)
    elseif self._state == "pending" then
        table.insert(self._waiters, { err = fn })
    end
    return self
end

function Promise:and_then(on_ok, on_err)
    local next_p = Promise.new()
    self:ok(function(v)
        if not on_ok then next_p:_resolve(v); return end
        local ok, res = pcall(on_ok, v)
        if ok then next_p:_resolve(res) else next_p:_reject(res) end
    end)
    self:err(function(e)
        if not on_err then next_p:_reject(e); return end
        local ok, res = pcall(on_err, e)
        if ok then next_p:_resolve(res) else next_p:_reject(res) end
    end)
    return next_p
end

-- Status accessors (read-only).
function Promise:state()       return self._state end
function Promise:is_pending()  return self._state == "pending"   end
function Promise:is_settled()  return self._state ~= "pending"   end

-- ── run / await -------------------------------------------------------------

--- Run `fn` inside a coroutine that understands `arbor.async.await`.
--- Returns the coroutine handle (mostly for testing).
function M.run(fn, ...)
    local args = { ... }
    local co
    local function step(...)
        local ok, yielded = coroutine.resume(co, ...)
        if not ok then
            if arbor and arbor.log and arbor.log.error then
                arbor.log.error("async.run error: " .. tostring(yielded))
            end
            return
        end
        if coroutine.status(co) == "dead" then return end
        if is_promise(yielded) then
            yielded:ok(function(v) step(true,  v) end)
                   :err(function(e) step(false, e) end)
        else
            step()
        end
    end
    co = coroutine.create(function() return fn(table.unpack(args)) end)
    step()
    return co
end

--- Yield the current coroutine until `promise` settles.
--- Returns `(value, nil)` on resolve, `(nil, err)` on reject.
--- Non-promise values pass through as `(value, nil)`.
function M.await(promise)
    if not is_promise(promise) then
        return promise, nil
    end
    if promise._state == "fulfilled" then return promise._value, nil end
    if promise._state == "rejected"  then return nil, promise._value end
    local ok, val = coroutine.yield(promise)
    if ok then return val, nil end
    return nil, val
end

-- ── Existing helpers --------------------------------------------------------

--- Returns a debounced version of `fn`.
--- The returned function resets the delay timer on every call;
--- `fn` fires only when no further calls arrive for `delay_ms`.
function M.debounce(fn, delay_ms)
    local timer_id = nil
    return function(...)
        local args = { ... }
        if timer_id then arbor.timer.cancel(timer_id) end
        timer_id = arbor.timer.after(delay_ms, function()
            timer_id = nil
            fn(table.unpack(args))
        end)
    end
end

--- Returns a throttled version of `fn`.
--- At most one call per `interval_ms` is allowed; intermediate calls are dropped.
function M.throttle(fn, interval_ms)
    local last = 0
    return function(...)
        local now = math.floor(os.clock() * 1000)
        if now - last >= interval_ms then
            last = now
            fn(...)
        end
    end
end

return M
