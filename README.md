# Rocket Tycoon

A terminal game about running a rocket company. You design engines,
find out the hard way what's wrong with them, and try to win launch
contracts before the money runs out.

```
┌ Rocket Tycoon ───────────────────────────────────────────────────────────────────────────────────┐
│  Aurora Launch      Dec 31, 2004      $220.4M      Teams: 3      ⏸ Paused                        │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
┌──────────────┐┌ Overview ────────────────────────────────────────────────────────────────────────┐
│ Overview     ││  Company:  Aurora Launch                                                         │
│ Engines      ││  Founded:  Jan 1, 2001                                                           │
│ Reactors     ││  Today:    Dec 31, 2004                                                          │
│ Rockets      ││  Elapsed:  1460 days                                                             │
│ Mfg          ││                                                                                  │
│ Contracts    ││  Money:    $220.4M                                                               │
│ Launches     ││                                                                                  │
│ Finance      ││  Eng. teams:      3                                                              │
│ Events       ││  Mfg. teams:      3                                                              │
│              ││  Engine projects: 2                                                              │
│              ││  Rocket projects: 1                                                              │
│              ││  Mfg. orders:     4                                                              │
│              ││  Rockets built:   1                                                              │
│              ││  Contracts:       3 available, 0 accepted                                        │
│              ││  Launches:        7                                                              │
│              ││  Reputation:      79                                                             │
│              ││                                                                                  │
│              ││  Economy:         Normal (+0%)                                                   │
└──────────────┘└──────────────────────────────────────────────────────────────────────────────────┘
┌ Recent ──────────────────────────────────────────────────────────────────────────────────────────┐
│  Dec 28, 2004: Engine built: BLV Upper                                                           │
│  Dec 3, 2004: Launch success: BLV-1 to Low Earth Orbit                                           │
│  Dec 3, 2004: Payment received: $31,870,000 for WeatherSat to LEO                                │
│  Dec 3, 2004: Flight arrived: BLV-1 at Low Earth Orbit                                           │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│  [Space] Pause  [1-3] Speed  [←→] Pane  [↑↓] Select  [S] Save  [F12] Report  [Q] Quit  [?] Keys  │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## The loop

You start with $200M, one engineering team, and a competitor with
deeper pockets than you.

- **Design engines.** Pick a cycle and propellants, assign teams, wait
  months. The engine arrives with **hidden flaws** — you know they're
  there, not how many or what they do.
- **Test to find them, revise to fix them.** Every revision costs time
  and partially resets your manufacturing learning curve. Flying an
  unrevised design usually ends in a fireball. That's normal.
- **Build rockets from your engines.** One engine family, two nozzles:
  the sea-level bell on the pad, the vacuum bell upstairs.
- **Bid for contracts.** Nobody tells you the customer's budget. Bid
  high and lose the work; bid low and fly at a loss. Every solicitation
  is visible from day one — what reputation buys you is whether a
  demanding customer picks *you* over the incumbent.

The world is rolled fresh each game — market demand, the technology
timeline, and the geopolitics that opens and closes whole categories
of contract. There's no strategy guide to look it up in.

## Playing it

Grab a binary from [Releases](https://github.com/aclough/rocket_game/releases),
or build it:

```sh
cargo build --release
./target/release/rocket_tycoon
```

Needs a stable Rust toolchain (developed on 1.98) and a terminal at
least 80×24. 100×32 or larger is more comfortable — panes clip rather
than wrap.

Two keys are worth knowing before anything else:

- **`?`** — every key for the tab you're on, plus the global ones.
  Nothing else in the game is hidden behind memorization.
- **`F12`** — writes a bug report. See below.

The game autosaves at the start of each month and when you quit, into
three rotating slots. `[S]` saves manually.

### Where your files live

| | |
|---|---|
| Linux, macOS, BSD | `~/.rocket_tycoon/` |
| Windows | `%APPDATA%\RocketTycoon\` |

Saves are in `saves/`, bug reports in `reports/`. Set
`ROCKET_TYCOON_DATA_DIR` to put the lot somewhere else.

Saves are pretty-printed JSON, deliberately — they stay greppable, and
a broken game is much easier to diagnose from one.

## Reporting a bug

Press **`F12`**. It writes two files and tells you where they went:

- a `.txt` report — version, git hash, seed, a summary of your company,
  and the full event log. This is the one to paste into chat.
- a `.json` save snapshot taken at the same moment.

The save is the actual bug report: it's what lets the problem be
reproduced. Send both if you can.

If the game **crashes**, the same thing happens automatically — the
panic handler gives your terminal back first, then writes an emergency
save (kept out of the autosave rotation so nothing can overwrite it)
and a report with the backtrace, and prints both paths. An hour of play
survives a panic.

## Platform support

- **Linux** — the development platform.
- **Windows** — supported. Built and tested in CI, and the interface
  has been checked by hand in Windows Terminal.
- **macOS** — builds and passes tests in CI, but nobody has run the
  interface on it. It should work; no promises.

## Development

```sh
cargo test                       # ~615 tests
cargo clippy --all-targets -- -D warnings
```

There's a headless simulation harness for balance work — it plays the
game with a scripted bot and prints metrics, no interface involved:

```sh
cargo run --release --bin simulate -- --seeds 1..20 --years 10 --policy basic
```

The balance guardrails live in `tests/sim_bands.rs` (200 seeds, marked
`#[ignore]` because it takes about a minute). Save-format compatibility
is pinned by a corpus of real saves from past versions in
`tests/saves/`, loaded by `tests/save_compat.rs`.

Design docs are in `Rocket_Tycoon.md` (what the game is for),
`ROADMAP.md` (what's next), and `docs/plans/` (how each piece got
built, including the arguments that were had along the way).

## Status

Pre-1.0. The simulation is in decent shape and the first fifteen
minutes have had real attention, but this has been played by roughly
one person. That's what the `F12` key is for.

## License

MIT — see [LICENSE](LICENSE).
