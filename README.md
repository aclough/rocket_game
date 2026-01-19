# Rocket Tycoon

A Rust/Godot space simulation game.

## Project Structure

- `godot/` - Godot 4.3+ project files
- `rust/` - Rust game logic library
- `build.sh` - Build script for Rust library

## Prerequisites

- Godot 4.3 or later
- Rust 1.70 or later
- Cargo

## Building

To build the Rust library:

```bash
./build.sh
```

Or manually:

```bash
cd rust
cargo build
```

For release builds:

```bash
./build.sh --release
```

## Verification

To verify the build and run automated tests:

```bash
./verify.sh
```

This runs 18 automated checks including:
- Project structure verification
- Rust compilation and tests (9 unit tests)
- File integrity checks
- Documentation verification

To run only Rust tests:

```bash
cd rust
cargo test
```

## Running

1. Build the Rust library (see above)
2. Open the `godot` directory in Godot Editor
3. Press F5 to run the game (main scene: `scenes/main.tscn`)

Or run one of the test scenes:
   - `scenes/main.tscn` - **Main game** - Launch rockets and try to reach orbit!
   - `scenes/rocket_launcher_test.tscn` - API test with console output
   - `scenes/test.tscn` - Basic Rust-Godot connection test

## Testing the Integration

### Basic Integration Test
The test scene (`godot/scenes/test.tscn`) includes a `TestNode` implemented in Rust. When you run it, you should see:
- Console output confirming Rust initialization
- A message from the Rust node's `test_connection()` function

### Rocket Launch Simulation Test
The rocket launcher test (`godot/scenes/rocket_launcher_test.tscn`) demonstrates the launch simulation API:
- Simple launch with success/failure
- Launch with descriptive messages
- Launch with stage-by-stage updates via signals
- Query stage information (names and failure rates)

### Main Game Features
The main game (`godot/scenes/main.tscn`) includes:
- Animated rocket sprite that ascends during launch
- Engine flame particle effects during flight
- Explosion particle burst with screen flash on failure
- Success sparkle effects with screen flash on reaching orbit
- Animated starfield background with twinkling stars
- Stage-by-stage progression with visual feedback
- Statistics tracking (attempts, successes, success rate)

## RocketLauncher API

The game includes a `RocketLauncher` node that simulates rocket launches to Low Earth Orbit.

Quick example:
```gdscript
var launcher = $RocketLauncher
launcher.stage_entered.connect(_on_stage_entered)
launcher.launch_completed.connect(_on_launch_completed)
launcher.launch_rocket_with_stages()
```

See [ROCKET_LAUNCHER_API.md](ROCKET_LAUNCHER_API.md) for complete documentation.

## Command Line Demo

You can also run a command-line demonstration of the launch simulation:

```bash
cd rust
cargo run --example launch_demo
```

This will show 5 launch attempts with stage-by-stage output and statistics from 1000 launches.

## Development

- Rust source code: `rust/src/`
- Godot scenes: `godot/scenes/`
- GDScript scripts: `godot/scripts/`
- API documentation: `ROCKET_LAUNCHER_API.md`
- Launch stages documentation: `LAUNCH_STAGES.md`
