#!/bin/bash
# Verification script for Rocket Tycoon prototype

echo "=========================================="
echo "Rocket Tycoon - Verification Script"
echo "=========================================="
echo ""

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PASS=0
FAIL=0

# Function to check test result
check_result() {
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ PASS${NC}: $1"
        ((PASS++))
    else
        echo -e "${RED}✗ FAIL${NC}: $1"
        ((FAIL++))
    fi
}

# Test 1: Check if Rust directory exists
echo "Checking project structure..."
test -d rust
check_result "Rust directory exists"

test -d godot
check_result "Godot directory exists"

# Test 2: Check for required files
test -f rust/Cargo.toml
check_result "Cargo.toml exists"

test -f godot/project.godot
check_result "project.godot exists"

test -f godot/rocket_tycoon.gdextension
check_result "GDExtension config exists"

# Test 3: Check Rust source files
test -f rust/src/lib.rs
check_result "lib.rs exists"

test -f rust/src/launcher.rs
check_result "launcher.rs exists"

test -f rust/src/rocket_launcher.rs
check_result "rocket_launcher.rs exists"

# Test 4: Check Godot files
test -f godot/scenes/main.tscn
check_result "main.tscn exists"

test -f godot/scripts/main.gd
check_result "main.gd exists"

test -f godot/assets/rocket.svg
check_result "rocket.svg exists"

# Test 5: Build Rust library
echo ""
echo "Building Rust library..."
cd rust
cargo build --quiet 2>&1 > /dev/null
check_result "Rust library builds"
cd ..

# Test 6: Check library file
test -f rust/target/debug/librocket_tycoon.so
check_result "Library file generated"

# Test 7: Run Rust tests
echo ""
echo "Running Rust tests..."
cd rust
cargo test --quiet 2>&1 > /dev/null
check_result "All Rust tests pass"
cd ..

# Test 8: Check for warnings
echo ""
echo "Checking for warnings..."
cd rust
WARNINGS=$(cargo build 2>&1 | grep -i warning | wc -l)
if [ $WARNINGS -eq 0 ]; then
    echo -e "${GREEN}✓ PASS${NC}: No compilation warnings"
    ((PASS++))
else
    echo -e "${YELLOW}⚠ WARN${NC}: $WARNINGS warnings found"
fi
cd ..

# Test 9: Verify documentation
test -f README.md
check_result "README.md exists"

test -f CURRENT_PROTOTYPE.md
check_result "CURRENT_PROTOTYPE.md exists"

test -f ROCKET_LAUNCHER_API.md
check_result "ROCKET_LAUNCHER_API.md exists"

# Summary
echo ""
echo "=========================================="
echo "Test Summary"
echo "=========================================="
echo -e "${GREEN}Passed: $PASS${NC}"
echo -e "${RED}Failed: $FAIL${NC}"
echo ""

if [ $FAIL -eq 0 ]; then
    echo -e "${GREEN}✓ All tests passed!${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Open godot/ directory in Godot editor"
    echo "  2. Press F5 to run the game"
    echo "  3. Test gameplay manually"
    exit 0
else
    echo -e "${RED}✗ Some tests failed!${NC}"
    echo "Please review the failures above."
    exit 1
fi
