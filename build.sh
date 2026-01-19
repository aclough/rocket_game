#!/bin/bash
set -e

echo "Building Rust library..."
cd rust
cargo build "$@"
cd ..

echo ""
echo "Build complete!"
echo "Library location: rust/target/debug/librocket_tycoon.so"
echo ""
echo "You can now open the Godot project in: ./godot"
echo "Test scene available at: scenes/test.tscn"
