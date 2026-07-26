# M5 Plan — First 15 minutes & release readiness

The last milestone before the MVP release. M1–M4 made the simulation
correct, varied, and appropriately harsh; M5 is about a stranger being
able to sit down, understand what to do, survive a crash, and tell us
what went wrong. Open questions marked **Q:** — please answer inline as
USER:.

---

## 1. Where the game sits today (measured)

Checked against the tree at `9e200ae` (M4 Task 5).

| Area | Current state |
|---|---|
| Build / tests | `cargo build` clean; **481 tests pass**, 0 fail |
| Clippy | 6 warnings (all trivial: `vec!`, redundant field names, `match`→`if let`, `.get().is_none()`), **0 errors** — the old `location.rs:55` error is already fixed |
| CI | **None.** No `.github/` at all |
| README | **None** |
| Help / keybindings | No help pane. `draw_help_bar` shows one static line of global keys; ~40 per-tab keys are undiscoverable |
| Onboarding | None. Startup screen offers New Game / load; then you're dropped on the Overview tab with $200M and one team |
| Save | Manual `[S]` only — **no autosave**, no format version, compat handled by ad-hoc backfills in `load_game` |
| Save location | `$HOME/.rocket_tycoon/saves/` — **`HOME` is unset on Windows**, so saves land in the CWD |
| Panic handling | **None.** A panic in raw mode leaves the terminal in the alternate screen with echo off, and the in-memory game is lost |
| Bug reporting | Nothing. `EventLog` is a ring buffer with a `total_pushed` counter — good raw material, but no export |
| Platform-specific code | Exactly one unix-ism: the `HOME` lookup in `save.rs:71`. Deps (ratatui 0.29, crossterm 0.28, rand, serde, toml) are all cross-platform |

### The undiscoverable-keys inventory

Every key currently bound, by tab. Nothing outside the global row is
documented anywhere in-game — this table is the argument for Task 2.

| Scope | Keys |
|---|---|
| Global | `Space` pause · `1`/`2`/`3` speed · `←`/`→` pane · `↑`/`↓` select · `S` save · `Q` quit |
| Engines | `n`/`N` `o` `r` `e` `b` `+`/`=` `-` |
| Reactors | `n`/`N` `r`/`R` `e`/`E` `+`/`=` `-` |
| Rockets | `n` `o` `r` `e` `m`/`M` `+`/`=` `-` |
| Mfg | `b` `m` `+`/`=` `-` |
| Contracts | `a` `b` `p`/`P` `r`/`R` `h`/`H` `Enter` |
| Launches | `l` `d`/`D` `f`/`F` `k`/`K` `u`/`U` `p` `Enter` |

Note the collisions across tabs (`r` = revise vs. rules vs. refresh,
`b` = build vs. bid, `e` = edit vs. engines) — a player who learns one
tab's vocabulary is actively misled on the next. This is a help-pane
problem, not necessarily a rebinding problem, but see **Q3**.

### What the first 15 minutes actually requires

`BasicPolicy` is the checked-in description of the known-good opening,
and it takes **~13 months of game time** to reach first launch. In
player terms the required sequence is:

1. Hire engineering + manufacturing teams (Overview / Mfg).
2. Start **two** engine projects — a sea-level booster and a vacuum
   upper (Engines, `n`).
3. Assign teams to them, wait out design.
4. Open the rocket designer, build a two-stage vehicle from those
   engines (Rockets, `n`).
5. Set an auto-build target so a vehicle exists (Mfg).
6. Test, discover flaws, revise — repeatedly.
7. Install bid rules / bid on a contract (Contracts).
8. Launch.

A new player is given none of this. Steps 2 and 6 in particular are
non-obvious: nothing tells you a rocket needs *two different* engine
projects, and nothing tells you that flying an unrevised design nearly
always ends in a fireball. That's the target of Task 3.

---

## 2. Proposed shape of the work

### 2.1 Help pane (Task 2)

A `?` key opens a modal listing global keys plus the *current tab's*
keys, with one-line descriptions.

The design risk is duplication: per CLAUDE.md, the key table and the
`handle_*_key` match arms must not become two sources of truth that
drift. Two ways to avoid it:

- **(a) Table-as-documentation + completeness test.** A `keys.rs`
  declares `&[(KeyDesc)]` per tab; a test asserts every key in the
  table is handled (feed each key to the handler on a fixture state and
  assert it isn't a no-op) — catches deletions but not additions.
- **(b) Table-as-dispatch.** Handlers become table lookups: each entry
  carries a description *and* an action fn. Impossible to drift, but
  it's a real refactor of ~700 lines of key handling across 10
  functions, and several handlers close over local mutable UI state in
  ways that don't map cleanly onto a fn-pointer table.

My lean: **(a)**, plus a hand-written note in `keys.rs` pointing at the
handlers. (b) is the "right" architecture but it's a big, risky diff to
land in the milestone whose whole point is release stability. See **Q3**.

### 2.2 Onboarding & automation (Task 3)

Proposal: a **"Next Steps" box on the Overview tab**, driven by a
data-driven rule list evaluated against `GameState` — the first 3
unsatisfied rules render, each with the tab and keystroke to act on.

```
 Next steps
  → Design a booster engine      Engines tab, [n]
  → Hire a manufacturing team    Mfg tab, [m]
  → Assign a team to BLV Booster Engines tab, [+]
```

Rules mirror the BasicPolicy sequence (which is *tested* to work), so
the advice is known-good rather than aspirational. It's advisory only —
non-modal, no pointer-grabbing, and it goes quiet once the player is
past the opening (last rule satisfied → box disappears, or shows a
one-line "you're running a rocket company now").

Plus a **first-run intro modal** on New Game: 10 lines on what the
company is, what the win condition looks like, and `?` for help.

This is deliberately *not* a scripted tutorial that gates progress —
those are expensive to build, brittle against balance changes, and this
audience (Hard SF Discord) will resent being walked. See **Q1**/**Q2**.

### 2.3 Save robustness (Task 4)

Four separate things bundled under one roadmap bullet:

1. **Format version field.** `GameState` gains `save_version: u32`
   (with `#[serde(default)]` → 0 for every existing save). `load_game`
   runs the existing ad-hoc backfills as explicit numbered migrations
   and stamps the current version. Turns "it happens to still
   deserialize" into something we can reason about.
2. **Save corpus + load-compat test.** Check in a handful of saves
   under `tests/saves/` — one per era (pre-M2, pre-M3 without
   competitors, current) — and a test that loads each and asserts
   invariants. This is the thing that catches a field rename breaking
   everyone's game between releases.
3. **Autosave.** Currently a player who plays for an hour and hits a
   panic loses everything. Proposal: autosave on month start to a
   rotating `<company>.auto1/2/3.json`, plus a save on clean quit.
4. **Panic handler.** `std::panic::set_hook` that (a) restores the
   terminal — `disable_raw_mode` + `LeaveAlternateScreen` — so the
   player gets a usable shell back, (b) writes an emergency save, (c)
   writes the crash report from 2.4, (d) prints where both went.
   Terminal restore is the highest-value item in this whole task.

### 2.4 Session / bug report dump (Task 5)

One keypress (proposal: `F12`, also invoked by the panic hook) writes
`~/.rocket_tycoon/reports/<company>-<date>.txt` containing:

- Version + git hash (via a build script or `env!` at compile time),
  platform, terminal size.
- World seed, current date, elapsed days, balance-config identity
  (default vs. loaded TOML).
- A state summary: money, reputation, teams, projects with status +
  flaw counts, inventory, active contracts, active flights.
- The full event log (ring buffer contents, ~all of it).
- Panic message + backtrace when invoked from the hook.
- A pointer to the accompanying emergency save.

The last one matters most: *the save is the bug report*. The text file
is what a player pastes into Discord; the save is what actually lets us
reproduce. See **Q5** on whether reports should be self-contained.

### 2.5 Windows (Task 6)

- **Save path.** Replace the `HOME` lookup with a platform-aware
  resolver: `%APPDATA%\rocket_tycoon\` on Windows, `$HOME/.rocket_tycoon`
  elsewhere. Either hand-rolled (`APPDATA` / `USERPROFILE` env vars, no
  new dependency) or via the `dirs` crate. Lean: hand-rolled, it's ~15
  lines and this project has kept its dependency list admirably short.
- **CI.** New `.github/workflows/ci.yml`: build + test on
  `ubuntu-latest` and `windows-latest`, plus `cargo clippy -- -D
  warnings` once the 6 warnings are cleaned. This is the first CI the
  repo has had.
- **Manual verification** of the things CI can't check: does the TUI
  actually render in Windows Terminal / PowerShell / cmd.exe, do the
  box-drawing and colors survive, do `+`/`=` and function keys arrive
  as expected through crossterm. See **Q6** — I can't do this part.

### 2.6 Vacuum variants per stage (Task 1 — new, from Q8)

**Today.** An engine project bakes its nozzle choice in at creation:
`EngineProject::new(..., use_vacuum_isp, ...)` reads `engine_baseline()`
and picks `isp_vac_s`/`exit_pressure_vac_pa` or the sea-level pair,
storing the result in `design`. A sea-level and a vacuum version of the
same engine are therefore two entirely separate projects — two design
programs, two NRE bills, two flaw sets, two test campaigns. That's why
the annoyance is real: the second one is mostly re-doing work you've
already done, and reality (Merlin 1D → Merlin 1D Vac, RL10 variants)
says a nozzle extension is a modest program, not a new engine.

**Proposed.** An engine project designs an engine *family*. The nozzle
variant is chosen where the engine is used — per stage, in the rocket
designer.

- `EngineProject` loses `use_vacuum_isp` at creation and gains
  `design_variant(vacuum: bool) -> EngineDesign`, deriving `isp_s`,
  `exit_pressure_pa`, and `needs_atmosphere` from the stored
  `(cycle, preset, scale)` — all three are already on the project.
- `Stage` gains `vacuum_variant: bool` (`#[serde(default)]`). Because
  `Stage.engine` is already a cloned snapshot, old saves keep the exact
  design they were built with and load with no migration.
- Engines whose baseline is `vacuum_only` (electric, solar sail, NTR,
  expander) offer no choice, exactly as now.
- The engine editor modal's vacuum checkbox goes away; the rocket
  designer's engine-pick modal gains the toggle, with the Isp delta
  shown live so the tradeoff is legible.

**Deliberately unchanged:** flaws, NRE, improvements, and tech
deficiencies stay on the project and are **shared across variants** —
same chamber, same turbopump, and it keeps the risk model simple.

**Third-party engines don't get variants.** A bought NK-33 is what it
is. That's a real asymmetry in favour of your own engine programs, and
it fits the existing "flaws in third-party parts can't necessarily be
fixed" flavour.

**Manufacturing.** `ManufacturingOrderType::Engine` will need to carry
the variant alongside `engine_id`/`revision` so the right nozzle is
built. Open sub-question: do the two variants share a learning curve?
Sharing is realistic (common core) and simpler; I lean share.

**Balance risk and the mitigation.** M4 landed dev spend to first
flight at $66M, a large part of which is *two* engine programs. Making
the second variant free would quietly undo a chunk of that. Hence:

- **Q10: variant qualification cost.** My proposal is a one-time work
  item charged the first time a project's *second* variant is used —
  sized as a fraction of the original design work (starting guess
  15–25%), swept against `sim_bands.rs` before it lands. Cheap enough
  that the annoyance is gone, expensive enough that "one engine, two
  nozzles" is a decision rather than a freebie. Alternatives: free (max
  convenience, real difficulty regression), or a full second project
  with a discount on shared work (most realistic, most machinery).
  Which? And should the qualification carry its own small flaw roll —
  the nozzle-extension-specific failure — or inherit the parent's flaws
  only?
  USER:  I think the UI simplicity dominates here and it should just be free at
  the expense of some realism.
  CLAUDE: Free it is — no qualification work item, no separate flaw roll,
  variants share the project's flaws/NRE/improvements/learning curve
  outright. That makes Task 1 substantially smaller: no new work-tracking
  state, no new balance knob, and the designer toggle is a pure UI
  affordance over data that already exists.
  Two consequences I'm accepting deliberately rather than silently:
  (1) the player-side difficulty regression is real — an upper stage
  that used to cost a second engine programme is now free — and I'll
  *measure* it with a `sim_bands.rs` run after the change rather than
  assume it away. BasicPolicy's two engines differ by propellant, so I
  expect the bands not to move at all; if they do, that's a finding to
  report, not something to quietly retune. (2) Sea-level and vacuum
  nozzles now share a learning curve, per my lean.

### 2.7 Packaging (Task 8)

- README: what the game is, screenshot, build instructions, how to
  report bugs (pointing at Task 4's dump).
- A release workflow producing Linux + Windows binaries on tag.
- LICENSE — needed before strangers touch it. See **Q7**.

---

## 3. Open questions

- **Q1: Onboarding shape.** Advisory "Next Steps" panel + one intro
  modal (my proposal above), or something more guided — a scripted
  first-contract walkthrough that gates progress until each step is
  done? The panel is ~1/4 the work and doesn't get stale when balance
  moves, but it does let a player wander off and lose 13 months of game
  time to a mistake they don't know they made.
  USER:  Next steps panel certainly.
  CLAUDE: Settled. I'm keeping the small first-run intro modal alongside it
  (one screen, dismissed with any key, never shown again) unless you'd
  rather have nothing modal at all — say so and I'll drop it and let the
  Next Steps panel carry the whole load.

- **Q2: Does the game ever act for the player?** E.g. New Game could
  start with one engineering *and* one manufacturing team already
  hired, or with the two engine projects pre-created and named, so the
  player's first decision is a real one (revise vs. fly) rather than
  bureaucratic setup. My lean: pre-hire a manufacturing team (pure
  friction removal, currently a hidden prerequisite nobody guesses),
  but let the player create their own engines — that's the signature
  mechanic and doing it for them steals the interesting part.
  USER:  One thing we have now is auto-assignment of idle manufacturing teams
  but not idle research teams.  Also, automatically going from testing to
  revising when a new flaw is discovered might be nice.  The thing about
  manufacturing teams is that they're just eating their paychecks until the
  first rocket design is done.  I think for our first target audiences having
  them do their own engines is best.
  CLAUDE: Agreed on all three, with one caveat on auto-revise. Checked
  against the code:
  - **Idle engineering teams.** Confirmed the asymmetry: `advance.rs:494`
    calls `auto_assign_idle_manufacturing_teams()` every day, and there is
    no engineering equivalent — `assign_idle_engineers` exists only inside
    `BasicPolicy`, so the bot gets a benefit the player doesn't. I'll add
    `auto_assign_idle_engineering_teams()` mirroring the mfg version
    (least-staffed active project first), plus the same
    `notified_*_idle` one-shot pause for when there's nothing to assign
    to. An idle engineering team is pure salary burn — the strategic
    decision is *re*assignment, never idleness.
  - **Auto Testing→Revising.** Worth doing but it needs to be a toggle,
    not unconditional. A revision bumps `EngineProject.revision`, and that
    number is carried onto manufacturing orders and inventory items
    (`ManufacturingOrderType::Engine { revision, .. }`,
    `RocketIntegration { revision, .. }`) — i.e. revising partially resets
    the production learning curve, exactly as Rocket_Tycoon.md describes.
    Auto-revising on every discovered flaw would silently churn that curve
    for a player who has a production run going. Proposal: a per-project
    `auto_revise` flag shown in the Engines/Rockets panes, default **on**;
    the player turns it off on a project they're currently mass-producing.
    Good news: revising does *not* block flying — `is_usable` accepts
    `Revising` — so the toggle only protects the learning curve, it never
    strands you without a vehicle.
  - **Manufacturing teams eating paychecks.** That reverses my Q2 lean —
    I'll *not* pre-hire one at game start. Instead the Next Steps panel
    raises "hire a manufacturing team" at the moment the rocket design
    reaches Testing, which turns a hidden prerequisite into timed advice
    without building the trap into the opening.

- **Q3: Help-pane architecture.** Option (a) declared table + a
  completeness test, or (b) full table-driven dispatch? Related: do you
  want a **rebinding pass** to reduce the cross-tab collisions (`r`,
  `b`, `e` meaning different things per tab), or is the help pane
  enough? My lean: (a) now, and leave rebinding alone — the collisions
  are survivable once documented, and churning keys right before
  release invalidates whatever muscle memory you personally have.
  USER:  Yeah, I think a is best.
  CLAUDE: Settled — (a), and no rebinding pass.

- **Q4: Autosave cadence and slots.** Month-start + on-quit with 3
  rotating slots (my proposal)? Or year-start (fewer writes, larger
  loss window)? Saves are pretty-printed JSON of the whole world — I
  should measure the file size before committing to a cadence, since
  a 5 MB save written every game-month at 3× speed is noticeable.
  USER:  For 3 slots the total of 15 MB isn't too much but yes we should
  measure.
  CLAUDE: Measured — I was wrong by more than an order of magnitude, saves
  are tiny. BasicPolicy runs, serialized whole:

  | Game years | Pretty JSON | Compact | Events held |
  |---|---|---|---|
  | 1 | 0.11 MB | 0.06 MB | 71 |
  | 3 | 0.15 MB | 0.08 MB | 278 |
  | 5 | 0.20 MB | 0.10 MB | 460 |
  | 8 | **0.27 MB** | 0.13 MB | 729 |

  Growth is ~20 KB per game-year and the event log is a ring buffer, so
  it's bounded. Three slots is under 1 MB, and a monthly write is free.
  Going with month-start + on-quit, 3 rotating slots as proposed, and
  keeping pretty-printing (saves stay greppable, which matters for bug
  reports). Caveat on the measurement: BasicPolicy parks no spacecraft
  (0 in every run) and builds one rocket design — a player hoarding
  spacecraft and a dozen projects could be several times this. Still
  nowhere near a problem.

- **Q5: Should bug reports be self-contained?** Options: (a) text
  report + separate save file, player sends both; (b) text report with
  the save embedded as base64 — one file, but huge and unreadable;
  (c) text report only, and we accept that some bugs aren't
  reproducible. Lean: (a), with the report naming the save path
  explicitly so players know to attach it.
  USER:  I agree with you on this.
  CLAUDE: Settled — (a), report names the save path explicitly.

- **Q6: Windows testing.** Do you have a Windows machine (or VM) to
  verify the TUI on? CI can prove it *builds and passes tests* on
  Windows, but not that the interface is usable. If not, the honest
  answer for 1.0 is "Windows is untested — Linux is the supported
  platform, Windows binaries provided as-is", and we solicit reports
  via the Task 4 dump. Also: is macOS in scope? crossterm handles it
  and it's a one-line CI addition, but it's another untested platform.
  USER:  I think I can get a Windows VM set up for this.
  CLAUDE: Good — that upgrades Windows from "provided as-is" to a
  supported platform, and Task 5's manual checklist (rendering in Windows
  Terminal vs. cmd.exe, box-drawing, colors, `+`/`=` and function keys
  through crossterm) becomes something you run on the VM once I've landed
  the save-path change. macOS went unanswered: adding `macos-latest` to
  the CI matrix is one line and costs nothing, so I'm including it as
  build+test only, with no usability claim in the README. Say if you'd
  rather leave it out entirely.

- **Q7: License and repo visibility.** ROADMAP says "long-term intent:
  open source". For the friends-and-Discord rollout, is the repo going
  public now (needs a LICENSE — MIT/Apache-2.0 dual is the Rust
  convention), or are we shipping binaries from a private repo and
  deciding later? This affects how the release workflow is built.
  USER:  This version should be open source, I was thinking MIT since patents
  shouldn't be a concern here.  At some point in the future, maybe with adding
  a GUI, I could keep back the new work from the open source release for
  a period of time but not now.
  CLAUDE: MIT it is — I'll add a standard `LICENSE` (MIT, "Copyright (c)
  2026 Andrew Clough" unless you want different attribution) and the SPDX
  line in `Cargo.toml`. Note MIT is fully compatible with the later plan:
  releasing 1.0 under MIT doesn't obligate you to release *future* work,
  so holding back a GUI version for a period stays open. The only thing
  MIT can't do is stop someone forking the 1.0 code — which for a game
  this size is a theoretical risk, not a real one.

- **Q8: Scope — fold in the TODO.txt UI polish?** The open items are
  all first-15-minutes surface: duplicate selection color in contracts,
  contract-view columns, non-vacuum default for engine design, and not
  showing flaw counts on completed designs. My lean: yes, as a small
  Task 0 — they're exactly the papercuts a new player hits, and the
  list is short. The Power items (battery defaults, launch/battery
  fencepost timing) are *not* cosmetic and I'd leave them out unless
  you think the fencepost bug is player-visible enough to block
  release.
  USER:  Yes.  Also, the vaccum / non-vaccum has been a big enough annoyance for
  me that I think we should do something about it as a seperate task in this.
  Existing engines have vaccum/non-vaccum varients, the Right Thing (TM) here is
  probably to let the user choose a vaccum/non-vacuum varient for each stage in
  a rocket.  My sense is that this is a Task sized ask but tell me if I'm wrong
  CLAUDE: You're right that it's one task, and it's a better-shaped fix
  than either TODO item — it *absorbs* both ("non-vacuum by default"
  stops being a question once the project doesn't pick a nozzle at all,
  and "allow vacuum variants easily" is the feature itself). But it's the
  biggest task in M5 and the only one that changes a fundamental data
  structure, so per CLAUDE.md here's the design to sign off on before I
  touch code. See **§2.7** below for the full write-up. Short version:

  - **The physics is already there.** `engine_baseline()` already returns
    *both* `isp_vac_s`/`isp_sl_s` and `exit_pressure_vac_pa`/
    `exit_pressure_sl_pa`. `EngineProject::new` just picks one with the
    `use_vacuum_isp` flag and bakes it into `design`. Nothing new to model.
  - **The data shape already fits.** `Stage.engine` is a *cloned*
    `EngineDesign` snapshot, not a reference to the project — so
    "variant chosen per stage" is exactly where the existing structure
    already puts it. Old saves keep their baked values and load unchanged.
  - **The real cost is balance, not code.** A free second variant halves
    the two-engine-program cost that M4 just spent five commits tuning
    (dev spend to first flight landed at $66M). I want a small one-time
    *variant qualification* work item rather than making it free — see
    §2.7 **Q10**.
  - **One reassuring finding:** BasicPolicy's two engines differ by
    *propellant* (kerolox booster, hydrolox upper), not just nozzle, so
    the bot wouldn't use variants and the M4 sim bands shouldn't move.
    The balance exposure is player-side only, which makes it much safer
    to land.

  I'd slot this as **Task 1** — early, so the checked-in save corpus
  (Task 3) is created against the final format and there's runway to
  re-check the bands before packaging.

- **Q9: What ends M5?** Concretely: is the release gate "these 6 tasks
  land", or is it "a friend plays for 30 minutes without getting stuck
  or crashing"? If the latter, we should plan for a feedback round
  *inside* M5 — build tasks 1–4, get it in front of one person, then
  fix what they hit before packaging. That's the version I'd argue for;
  onboarding is the one thing in this project that can't be validated
  by a test suite or a 200-seed sweep.
  USER:  More the later.  I'll share it on a Discord with just a few people
  I know and get feedback there first.
  CLAUDE: Then M5 has a seam in the middle, and §4 is restructured around
  it: **Block A** (playable build) → you share it with a few people →
  **Block B** (fix what they hit, then package). The bug-report dump moves
  *into* Block A, since it's the instrument that makes the feedback round
  produce something actionable rather than "it broke somehow".

---

## 4. Tasks (each with its own commit + your approval)

Per Q9, M5 has a seam in the middle: build a playable, instrumented,
Windows-capable build; put it in front of a few people; then fix and
package. Ordering rationale after the list.

### Block A — the build you hand to friends

0. **TODO.txt UI polish** — duplicate selection colour in contracts,
   contract-view columns, and no flaw count on completed designs.
   (The fourth item, "non-vacuum by default", is deleted rather than
   fixed — Task 1 removes the question.) Small warm-up in the same draw
   code Tasks 2–3 will be in.
1. **Vacuum variants per stage** — §2.6. `EngineProject::design_variant`,
   `Stage.vacuum_variant`, designer toggle with live Isp delta, variant
   on the manufacturing order, qualification cost per **Q10**, then a
   `sim_bands.rs` re-check. Biggest task and the only data-structure
   change; goes early so Task 4's save corpus is built against the final
   format. **Blocked on Q10.**
2. **Help pane** — `keys.rs` table + `?` modal + completeness test.
   Also updates the static help bar to advertise `?`.
3. **Onboarding & automation** — Next Steps rule engine on Overview,
   first-run intro modal, `auto_assign_idle_engineering_teams()` with
   its idle-pause, and the per-project `auto_revise` toggle (default on).
   Headless render tests for the panel in the states it should and
   shouldn't appear.
4. **Save robustness** — `save_version` + numbered migrations, checked-in
   save corpus + load test, month-start/on-quit autosave with 3 rotating
   slots, panic hook with terminal restore and emergency save.
5. **Session/bug report dump** — `F12` + panic-hook path, with a test
   that the report generator runs on a mid-game state without panicking
   (a crash handler that crashes is worse than none).
6. **Windows support + CI** — platform-aware save dir (`%APPDATA%`)
   with migration of existing `~/.rocket_tycoon` saves,
   `.github/workflows/ci.yml` (Linux + Windows + macOS build/test),
   clippy warnings cleaned so `-D warnings` can gate. Then your VM pass
   on the actual TUI. This is in Block A rather than after it because
   the friends round probably includes Windows players.

**→ hand to a few people on Discord.**

### Block B — after feedback

7. **Feedback fixes** — scope unknown by definition; the Task 5 dumps
   are what make this tractable. Expect most of it to be onboarding
   wording and papercuts rather than simulation bugs.
8. **Packaging** — README with screenshot, MIT LICENSE, release
   workflow producing tagged Linux/Windows binaries.

Ordering notes: Task 1 before Task 4 (save corpus wants the final
format); Task 2 before Task 3 (Next Steps entries reference keys the
help table names, so one source of truth for key descriptions); Task 5
before the friends round (it's the instrument that makes the round
useful). Tasks 0 and 6 are independent and can move.

Housekeeping to fold in along the way (from ROADMAP's hygiene list):
archive `m4_plan.md` and this file into `docs/plans/` when M5 closes;
the root should keep only ROADMAP.md, TODO.txt, Rocket_Tycoon.md, and
CLAUDE.md.

Not in scope: the ROADMAP "Later" loops (depots, probes, comms,
tourism), competitor depth, and any further balance work — M4's bands
are the shipping numbers, with the single exception of the Task 1
re-check (a mechanics change has to prove it didn't move them).

---

## 5. Still open

All questions answered; work started 2026-07-26. Three footnotes went
unanswered and I'm proceeding on the defaults I stated — none of them
bite before Task 3, so there's time to reverse any of them:

- **Q1 footnote** — keeping the one-screen first-run intro modal
  (Task 3).
- **Q6 footnote** — `macos-latest` included in the CI matrix as
  build+test only, no usability claim (Task 6).
- **Q7 footnote** — LICENSE reading "Copyright (c) 2026 Andrew Clough"
  (Task 8).

- **Q11 (Task 0)** — "Don't list flaw count when a design is completed":
  I can't find the screen you mean. Every flaw count I can locate is
  already gated on there being live, discovered, unfixed flaws, and a
  completed revision *removes* flaws from the vec
  (`engine_project.rs:580`), so the count goes to zero and the block
  stops rendering on its own. The places a flaw count appears at all:
  1. Engines/Rockets/Reactors panes, selected project: `Flaws: N
     discovered` + the per-flaw list (`draw.rs` 384/627/861) — gated on
     `discovered_flaw_count() > 0`.
  2. Project status line while revising: `Revising N flaw(s), M
     improvement(s)` (`draw.rs` 289/542/738) — this one *can* read
     "Revising 0 flaw(s), 2 improvement(s)" when the flaws are done but
     improvements aren't. Is that the one?
  3. Transient status messages after pressing `r` (`mod.rs` 1004/1112/
     1153).
  Which of these, and what should "completed" mean — a project whose
  discovered flaws are all revised away, or something else?
  USER:  It's in events, when a rocket is finished with its design this
  prints:  Dec 10, 2001: Rocket design complete: Smol (5 flaws)
  CLAUDE: Found it — and it's an information leak, not a cosmetic
  nit, so worth recording. The three `*DesignComplete` events carried
  `flaw_count = self.flaws.len()` (`rocket_project.rs:91`,
  `engine_project.rs:542`, `reactor_project.rs:254`) — the **total**
  count, every flaw of which is undiscovered at that instant. So the
  event handed the player the exact number that the test/discover loop
  exists to conceal, letting them compute precisely how long to keep
  testing and when to stop. `total_flaw_count()` is even commented
  "hidden from player — for testing only". This directly undercuts
  M4 Task 3's rush-risk design.
  Fixed by deleting `flaw_count` from the three `GameEvent` variants
  *and* the three internal `*WorkEvent::DesignComplete` variants that
  fed them, so the number can't leak back by accident rather than
  merely being unprinted. The M1 sim harness reads undiscovered flaw
  counts from project state (`sim.rs:141`), not from these events, so
  instrumentation is unaffected. Two guard tests in `event.rs`: one
  pinning the rendered strings, one confirming pre-M5 saves (which have
  the field in their serialized event log) still deserialize.

### Progress

- [x] Task 0 — TODO.txt UI polish
- [ ] Task 1 — vacuum variants per stage
- [ ] Task 2 — help pane
- [ ] Task 3 — onboarding & automation
- [ ] Task 4 — save robustness
- [ ] Task 5 — session/bug report dump
- [ ] Task 6 — Windows support + CI
- [ ] → friends round on Discord
- [ ] Task 7 — feedback fixes
- [ ] Task 8 — packaging
