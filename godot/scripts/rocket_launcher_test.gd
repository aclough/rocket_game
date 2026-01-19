extends Node

# Test script for RocketLauncher API

func _ready():
	print("\n=== RocketLauncher API Test ===\n")

	# Test 1: Simple launch
	test_simple_launch()

	# Test 2: Launch with message
	test_launch_with_message()

	# Test 3: Launch with stage callbacks
	test_launch_with_stages()

	# Test 4: Query stage information
	test_stage_info()

func test_simple_launch():
	print("Test 1: Simple Launch")
	var launcher = $RocketLauncher

	var success = launcher.launch_rocket()
	if success:
		print("  Result: SUCCESS!")
	else:
		print("  Result: FAILURE")
	print()

func test_launch_with_message():
	print("Test 2: Launch with Message")
	var launcher = $RocketLauncher

	var message = launcher.launch_rocket_with_message()
	print("  " + message)
	print()

func test_launch_with_stages():
	print("Test 3: Launch with Stage Updates")
	var launcher = $RocketLauncher

	# Connect to signals
	launcher.stage_entered.connect(_on_stage_entered)
	launcher.launch_completed.connect(_on_launch_completed)

	# Launch!
	launcher.launch_rocket_with_stages()

	# Wait a moment for signals to process
	await get_tree().create_timer(0.1).timeout
	print()

func test_stage_info():
	print("Test 4: Stage Information")
	var launcher = $RocketLauncher

	var stage_count = launcher.get_stage_count()
	print("  Total stages: %d" % stage_count)
	print("  Stages:")

	for i in range(stage_count):
		var name = launcher.get_stage_description(i)
		var failure_rate = launcher.get_stage_failure_rate(i)
		print("    %d. %s (%.1f%% failure)" % [i+1, name, failure_rate * 100])
	print()

func _on_stage_entered(stage_name: String):
	print("  ✓ %s - PASSED" % stage_name)

func _on_launch_completed(success: bool, message: String):
	print("  RESULT: %s" % message)
