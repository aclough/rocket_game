# CLAUDE.md

# Overview

This will be a Rust/Godot video game.

See 'Rocket Tycoon - 1.0.md' for what the final game will be like.  The godot stuff will go in ./godot and the rust
stuff in ./rust

# Current Iteration

In the very first iteration the player will be able to try launching a rocket to LEO.  The player will be preseted with
the option to launch their rocket.  It will be a two stage rocket trying to reach LEO.  As it flieds it will pass
through several stages like 'ignition', 'Max-Q', etc.  At each it has a chance to explode.  If it reaches orbit or
explodes show the user and then let them try again.
