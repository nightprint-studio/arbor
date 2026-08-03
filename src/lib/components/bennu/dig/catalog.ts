/**
 * The `.dig` language catalog — GENERATED, do not edit by hand.
 *
 *   node scripts/gen-dig-catalog.mjs --geode <path-to-geode> --lang en
 *
 * Copied from geode's own translation files (`content/core/i18n/en/{lang,keywords,symbols,methods}.toml`),
 * which are the authoritative help text for the language: the keys of `lang.toml` are
 * the vocabulary itself (a geode test pins the two together), and the first line of each
 * entry is its **signature** — that formatting is a contract over there, and
 * `dig-catalog.ts` relies on it to split "signature" from "explanation".
 *
 * Re-run the generator after a geode change that adds or rewords a builtin.
 *
 * Contents: 49 builtins · 22 keywords ·
 * 5 namespaces (26 members) · 5 collection methods.
 */

import type { DigCatalog } from './dig-catalog';

export const DIG_CATALOG: DigCatalog = {
  "language": "en",
  "builtins": {
    "bot": "bot()\nReturns the mole the program is currently running on.",
    "scan": "scan()\nWhat is in the cell the mole is FACING: `Crystal(kind)` if there is a\ncrystal, `Empty` if it is empty or if there is no cell in front (you are standing on a\ncell instead of in a gap).\n\n    if scan() == Empty:\n        seed(Crystal.Amethyst)",
    "block_size": "block_size()\nHow many crystals a `harvest()` on the cell you are FACING would detach.\n\n    if block_size() > 1:\n        print(\"a block: worth more, but the corridor between is sealed\")\n\nThree answers:\n- `0` — nothing to harvest: empty cell, rock, or you are not facing a cell\n  (you are ON a cell instead of in a gap).\n- `1` — a single crystal. The normal case.\n- `N` — a welded BLOCK of N cells, which `harvest()` detaches all at once.\n\nA block pays more per cell, so this number tells you what the move is worth.\n\n⚠️ `block_size() > 1` also means THERE IS A WALL THERE: a block fills the corridors\nbetween its cells, and you cannot walk through. Asking first saves you a `move()` that\nreturns `false` without explaining why.\n\nIt looks at neither ripeness (`is_ripe()`) nor the tool you carry: it counts cells.\nA block of four unripe gems is still a block of four.",
    "is_ripe": "is_ripe()\nIs there a RIPE crystal in front? `true` if the cell you face holds a\nfully grown crystal, `false` if it is empty, rock, unripe, or if you are not facing\na cell (you are standing on a cell instead of in a gap).\n\n    if is_ripe():\n        harvest()\n\nIt looks at the WORLD, not at your gear: if a ripe diamond is in front and you hold the\npickaxe, it still answers `true` — then `harvest()` fails and tells you in the console\nwhich tool you need. To know whether you can REALLY harvest it, the answer is\n`harvest()`, not this.\n\n⚠️ Use it to CHOOSE, not to wait: `while is_ripe() == false: pass` never\nends if that crystal is frozen by its neighbours. Skip the cell and come back.",
    "ripe_left": "ripe_left()\nHow many TICKS you have left to harvest what is in front of you, before it rots.\n\n    if is_ripe():\n        if ripe_left() < 6:\n            harvest()\n\nSome crystals are PERISHABLE: they ripen, stay harvestable for a while, and then\nrot — the cell empties and you get nothing. This function tells you how long is\nleft, for the crystal you are FACING.\n\n⚠️ When there is no deadline it answers a HUGE NUMBER (one million), not zero and\nnot `-1`. That way `if ripe_left() < 6:` is right in every case with a single\ncomparison: it is false for a crystal that never rots, for an empty cell, for rock\nand for an unripe crystal — all of which are different ways of saying \"no rush\".\n\nThe strategy that pays is not standing there watching it ripen: it is alternating it\nwith a crystal that SPEEDS UP its neighbours, seeding the whole grid, and harvesting\non the way back while the first ones are still inside their window.\n",
    "clock": "clock()\nThe WORLD TICK COUNT since the game began.\n\n    let start = clock()\n    # ...\n    print(clock() - start)   # how many ticks the round took\n\nIt is the FIELD clock: it always rises, even with no program running, because crystals\ngrow anyway. Use it to measure how long something takes, and to sync with crystals that\nhave a harvest window (`tick % period`).\n\n⚠️ It is NOT the twin of `set_tick`. That one sets how many INSTRUCTIONS PER SECOND your\ncode runs; this counts WORLD ticks, a different unit at a different speed. It is called\n`clock` precisely so the two do not look related.\n",
    "pos": "pos()\nWhere the mole is, in fine coordinates: `[fx, fz]`.\n\nSame ones as `width()`/`height()`: a position is a CELL when fx and fz are\nboth even, otherwise it is a gap. From a gap you seed and harvest\non the cell you are facing.\n\n    let p = pos()\n    println(p[0])          # no purchase needed: indexing does NOT require `lists`\n\nIt exists so you never count steps by hand: a blocked `move` returns `false`\nwithout moving anything, and a tally kept in your head from there on is wrong.",
    "reset": "reset()\nClears the field and returns the mole to the start, WITHOUT stopping the program.\n\nThat is the difference from the Stop button: that one is the player's handle, this is a\nmove made by the code. Use it to retry without leaving the program.\n\n    reset()\n    seed_everything()",
    "home": "home()\nBrings the mole back to the starting point, digging (it takes a while).\nOn arrival it faces North.",
    "width": "width()\nHow wide the **dug** field is, in fine coordinates (cells + gaps). Not the grid\nbounds: rock you do not own yet does not count, because you cannot go there.\nIt grows as the dig crew opens new cells.\n\n    while pos()[0] + 2 < width():\n        move(East)",
    "height": "height()\nHow tall the **dug** field is, in fine coordinates (cells + gaps). Like width():\nit measures where you can go, not the box the cavern sits in.",
    "look": "look(direction)\nLooks in a direction and reports the adjacent cell.",
    "move": "move(direction)\nMoves the mole one step in the given direction.",
    "dig": "dig(direction)\nDigs the adjacent cell in the given direction.",
    "prepare": "prepare(target)\nPrepares the `target` site for crystal nucleation.",
    "harvest": "harvest()\nHarvests the crystal in the cell you are FACING, and answers \"did I harvest\nanything?\".\n\n    if is_ripe():\n        harvest()\n\nThree cases, and two of them bite:\n- EMPTY cell (or rock, or you are not in a gap): `false`, and nothing happens.\n- UNRIPE crystal: it is torn out and THROWN AWAY. The cell empties, nothing enters\n  the store, and the answer is `false`. It is not left there waiting: you lose it.\n- RIPE crystal with the right tool: `true`, and it enters the store under its name.\n\nEACH CRYSTAL NEEDS ITS TOOL (`equip`): with the wrong one it answers\n`false` and the console tells you which one is needed.\n\n⚠️ Do NOT write `while harvest() == false: pass`. It destroys what you meant to\nharvest, and never ends: a badly surrounded crystal can stay unripe\nforever. Ask `is_ripe()` first, and if it is not ready move on and come back later.",
    "print": "print(value)\nPrints `value` to the player's console.",
    "println": "println(value)\nPrints `value` to the console, on a new line.",
    "log": "log(value)\nWrites `value` to the console (like `print`: `log` is NOT a module, it has no `.info`).",
    "out": "out(value)\nWrites `value` to the console. Historical alias of `print`.",
    "remaining": "remaining()\nHow many resources are still left on the grid.",
    "collected": "collected()\nHow many resources you have collected so far.",
    "sense": "sense()\nWhat lies under the pod: Gem(kind) if a crystal, otherwise Empty.",
    "random": "random(from, to)\nRandom number: random() float [0,1), random(to) integer [0,to), random(from,to).",
    "North": "North\nDirection: north (upwards).",
    "East": "East\nDirection: east (to the right).",
    "South": "South\nDirection: south (downwards).",
    "West": "West\nDirection: west (to the left).",
    "set_tick": "set_tick(n)\nSpeed of the **code**: instructions per second. From 1 to 100000; out of range it is\nclamped. You can also say Tick.MIN_VALUE / Tick.MAX_VALUE.\n\n    set_tick(60)\n    set_tick(Tick.MAX_VALUE)",
    "set_speed": "set_speed(n)\nSpeed of the **actions**: cells per second (how fast the pod slides during a\nmove). From 0.25 to 24 — the minimum is not zero: at zero it would never arrive. You can\nalso say Speed.MIN_VALUE / Speed.MAX_VALUE.\n\n    set_speed(8)\n    set_speed(Speed.MAX_VALUE)",
    "set_wrap": "set_wrap(on)\nTurns the toroidal grid on or off (edges that wrap around).",
    "neighbors": "neighbors(cell)\nReturns the cells adjacent to `cell`.",
    "distance": "distance(a, b)\nReturns the grid distance between `a` and `b`.",
    "path_to": "path_to(target)\nComputes a path from the mole to `target`.",
    "range": "range(count)\nThe sequence of integers to walk with `for`: `range(n)` goes from 0 to n-1,\n`range(from, to)` from `from` to `to`-1 (the upper bound is excluded). You need it because a number\nis not iterable: `for i in 254` does not count to 254.\n\n    for i in range(4):\n        out(i)\n\n    for i in range(2, 5):\n        out(i)",
    "conditions": "conditions()\nReturns the environment of the current cavity (temp, humidity, room…).",
    "nucleate": "nucleate(crystal)\nAttempts to nucleate `crystal`; returns a Result( .ok .reason ).",
    "expose": "expose(elem)\nExposes an `elem` vein as a substrate for nucleation.",
    "goto": "goto(pos)\nTakes the mole all the way to position `pos`.",
    "surface": "surface()\nBrings the mole back to the surface.",
    "deposit": "deposit()\nDeposits the collected load.",
    "sell": "sell(crystal, count)\nSells up to `count` crystals of that kind and returns the takings in dollars.\n\n    let takings = sell(Crystal.Quartz, 10)",
    "sell_all": "sell_all(crystal)\nSells **all** crystals of that kind and returns the takings in dollars.\n\n    let takings = sell_all(Crystal.Amethyst)",
    "len": "len(collection)\nHow many items a list, a map or a text has (characters, not bytes).\n\n    for i in range(len(load)):\n        out(load[i])",
    "crystals": "crystals()\nThe list of ALL crystal kinds, as Crystal.X symbols. It saves you from\nlisting them by hand: you iterate over it, and a crystal added by a mod joins on its\nown. They are the same symbols you write yourself, so seed(c)/sell(c, n) accept\nthem.\n\n    for c in crystals():\n        turn(North)\n        seed(c)",
    "turn": "turn(direction)\nTurns the pod towards a direction, on the spot. It does not move: it only changes where\nit looks (and therefore which cell `seed`/`harvest` act on).\n\n    turn(North)",
    "equip": "equip(tool)\nMounts a tool on the snout, at once. Each crystal wants its own.\n\n    equip(Tool.Pick)",
    "seed": "seed(crystal)\nPlants a crystal in the cell the pod is facing. It must be done from a gap,\nfacing the cell.\n\n    turn(North)\n    seed(Crystal.Amethyst)",
    "stock": "stock(what)\nHow many you have: a crystal in the warehouse, your dollars, or your free digs.\nAlways returns a number — zero is an answer, not an error.\n\n    if stock(Crystal.Amethyst) < 10:\n        seed(Crystal.Amethyst)\n    if stock(Item.Money) > 500:\n        println(\"time to buy\")\n\n`Item.` must be spelled out; a crystal name may also be written bare."
  },
  "keywords": {
    "from": "from <module> import <names>\nTakes names from another file: **only** the ones you write come in, and they are used\nbare. For the rest you need `import <module>`.\n\n    from utils import dig_down, come_up\n    dig_down(3)",
    "import": "import <module>\nBrings in another file under its **name**: its things are called `module.name`,\nbare they are invisible. To have them bare, `from <module> import <names>`.\n\n    import utils\n    utils.dig_down(3)",
    "let": "let <name> = <value>\nDeclares a new variable. To change it later, `let` is not repeated.\n\n    let energy = 10\n    energy = energy - 1",
    "fn": "fn <name>(<parameters>):\nDefines a function: a piece of code you give a name to so you can reuse it.\n\n    fn dig_down(how_many):\n        for i in range(0, how_many):\n            dig(South)",
    "struct": "struct <Name>:\nDefines a type with its fields (and its methods). The fields are **only** the ones\ndeclared here: you do not add more along the way.\n\n    struct Floor:\n        bottom\n        kind",
    "if": "if <condition>:\nRuns the block only if the condition is true.\n\n    if energy > 0:\n        move(North)",
    "elif": "elif <condition>:\nAnother condition, tried only if the earlier ones were false. It goes after an `if`.\n\n    if x > 0:\n        out(1)\n    elif x < 0:\n        out(-1)",
    "else": "else:\nThe block to run when none of the conditions above is true.\n\n    if full:\n        deposit()\n    else:\n        dig(South)",
    "while": "while <condition>:\nRepeats the block as long as the condition stays true. Careful: if it never turns\nfalse, the loop never ends.\n\n    while remaining() > 0:\n        harvest(Tool.Pick, bot())",
    "for": "for <name> in <sequence>:\nRepeats the block once for each item of the sequence.\n\n    for cell in scan(2):\n        out(cell)",
    "in": "<value> in <sequence>\nTwo uses: it tells `for` what to walk, and on its own it asks \"is it inside?\".\n\n    for i in range(0, 4):\n        out(i)\n\n    if Crystal.Ruby in load:\n        deposit()",
    "match": "match <value>:\nPicks a branch based on the value — more readable than a chain of `if`.\nThe `_` branch takes everything else.\n\n    match sense():\n        Gem g -> harvest(Tool.Pick, g)\n        _ -> dig(South)",
    "return": "return <value>\nLeaves the function at once and gives back the value (which may also be missing).\n\n    fn double(n):\n        return n * 2",
    "pass": "pass\nDoes nothing. It fills a block that must exist but is still empty.\nIt does not stop execution (that is `return`) and does not even cost a tick.\n\n    fn to_be_written():\n        pass",
    "continue": "continue\nJumps to the next turn of the loop, without running the rest of the block.\n\n    for c in scan(2):\n        if c == none:\n            continue\n        out(c)",
    "break": "break\nLeaves the loop at once, without waiting for the condition to turn false.\n\n    while true:\n        if full:\n            break\n        dig(South)",
    "and": "<a> and <b>\nTrue only if **both** conditions are true.\n\n    if energy > 0 and not full:\n        dig(South)",
    "or": "<a> or <b>\nTrue if **at least one** of the two conditions is true.\n\n    if full or energy == 0:\n        surface()",
    "not": "not <a>\nFlips a condition around: true becomes false, and the other way round.\n\n    if not full:\n        dig(South)",
    "true": "true\nThe true value.\n\n    while true:\n        dig(South)",
    "false": "false\nThe false value.\n\n    let active = false",
    "none": "none\nThe \"nothing\": the value of what is not there — a function without `return`, a field\nnever assigned, an empty cell.\n\n    if look(North) == none:\n        move(North)"
  },
  "namespaces": {
    "Tool": {
      "about": "Tool.X\nThe tools to mount with `equip`: every crystal wants its own.",
      "members": {
        "Pick": "Tool.Pick\nPickaxe: for soft crystals.",
        "Drill": "Tool.Drill\nDrill: for hard/metallic crystals.",
        "Laser": "Tool.Laser\nLaser: for the hardest/most precious crystals."
      }
    },
    "Tick": {
      "about": "Tick.X\nThe limits of the **code** rate (instructions/sec), for `set_tick`.",
      "members": {
        "MIN_VALUE": "Tick.MIN_VALUE\nThe slowest rate: 1 instruction per second.",
        "MAX_VALUE": "Tick.MAX_VALUE\nThe fastest rate: 100000 instructions per second."
      }
    },
    "Speed": {
      "about": "Speed.X\nThe limits of the **action** speed (cells/sec), for `set_speed`.",
      "members": {
        "MIN_VALUE": "Speed.MIN_VALUE\nThe slowest: 0.25 cells per second. It is not zero: at zero it would never arrive.",
        "MAX_VALUE": "Speed.MAX_VALUE\nThe fastest: 24 cells per second."
      }
    },
    "Crystal": {
      "about": "Crystal.X\nThe crystal kinds to plant with `seed`.",
      "members": {
        "Amethyst": "Crystal.Amethyst\nAmethyst: purple prisms (pickaxe).",
        "Diamond": "Crystal.Diamond\nDiamond: precious octahedra (laser).",
        "Pyrite": "Crystal.Pyrite\nPyrite: metallic cubes (drill).",
        "Sulfur": "Crystal.Sulfur\nSulfur: rosettes of waxy yellow points (pickaxe).",
        "Hematite": "Crystal.Hematite\nHematite: metallic rosettes (drill).",
        "Malachite": "Crystal.Malachite\nMalachite: green nodules (pickaxe).",
        "Geode": "Crystal.Geode\nGeode: a shell with purple druses (drill).",
        "Quartz": "Crystal.Quartz\nQuartz: pale solitary points (drill).",
        "Halite": "Crystal.Halite\nRock salt: translucent pink cubes (pickaxe).",
        "Graphite": "Crystal.Graphite\nGraphite: black metallic points — a black amethyst (pickaxe).",
        "Calcite": "Crystal.Calcite\nCalcite: honey cleavage rhombs (pickaxe).",
        "Fluorite": "Crystal.Fluorite\nFluorite: glowing green cubes; walking past one makes you think and move faster (pickaxe). From the example pack.",
        "Magnetite": "Crystal.Magnetite\nMagnetite: black iron octahedra; once ripe they pull neighbouring crystals in and seal the corridors between them (drill).",
        "Labradorite": "Crystal.Labradorite\nLabradorite: grey-blue stone that flashes blue and gold (drill).",
        "Ruby": "Crystal.Ruby\nRuby: fire-red corundum, extremely hard (laser).",
        "Sapphire": "Crystal.Sapphire\nSapphire: the same mineral as ruby, blue and zoned (laser).",
        "Emerald": "Crystal.Emerald\nEmerald: green beryl, prismatic and full of inclusions (drill)."
      }
    },
    "Item": {
      "about": "Item.X\nWhat you own that is **not** a crystal: ask `stock` for it, you cannot seed it.",
      "members": {
        "Money": "Item.Money\nThe dollars in your pocket. `stock(Item.Money)`.",
        "Digs": "Item.Digs\nThe free digs left to spend, granted by the tree. `stock(Item.Digs)`."
      }
    }
  },
  "methods": {
    "list": {
      "append": "list.append(x)\nAppends `x` at the end. It is `l = l + [x]` written better — and faster.\nIt wants a **variable**: `[1,2].append(3)` would write to a copy nobody reads back.\n\n    let found = []\n    for c in scan(2):\n        found.append(c)",
      "remove": "list.remove(i)\nRemoves the item at position `i`. The index must be valid: lists have no holes.\n\n    let l = [1, 2, 3]\n    l.remove(0)",
      "has": "list.has(x)\nIs `x` in there? It is `x in list` written with a dot.\n\n    if found.has(Crystal.Amethyst):\n        out(1)"
    },
    "map": {
      "remove": "map.remove(key)\nRemoves a key. Removing one that is not there **is not an error**: it is the request\n\"make sure it is gone\". Without it, a map could only grow.\n\n    let m = {\"a\": 1}\n    m.remove(\"a\")",
      "has": "map.has(key)\nIs that key there? It is `key in map` written with a dot.\n\n    if visited.has(pos):\n        continue"
    }
  }
};
