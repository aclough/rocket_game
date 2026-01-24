extends Control

# UI references
@onready var launch_button = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/LaunchButton
@onready var design_button = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/DesignButton
@onready var status_panel = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/StatusPanel
@onready var stage_list = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/StatusPanel/MarginContainer/VBox/ScrollContainer/StageList
@onready var result_panel = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/ResultPanel
@onready var result_label = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/ResultPanel/MarginContainer/VBox/ResultLabel
@onready var message_label = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/ResultPanel/MarginContainer/VBox/CenterContainer/MessageLabel
@onready var try_again_button = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/ResultPanel/MarginContainer/VBox/TryAgainButton
@onready var subtitle = $MarginContainer/VBoxContainer/HeaderContainer/Subtitle

# Main content container (to hide when showing design screen)
@onready var main_content = $MarginContainer
@onready var rocket_visual = $RocketVisual

# Rocket launcher reference
@onready var launcher = $RocketLauncher

# Rocket visual reference
@onready var rocket_sprite = $RocketVisual/RocketSprite

# Screen effects reference
@onready var screen_effects = $ScreenEffects

# Design screen (loaded dynamically)
var design_screen: Control = null
var design_screen_scene = preload("res://scenes/design_screen.tscn")

# Testing screen (loaded dynamically)
var testing_screen: Control = null
var testing_screen_scene = preload("res://scenes/testing_screen.tscn")

# State tracking
var attempt_count = 0
var success_count = 0
var has_custom_design = false
var last_launch_success = false

func _ready():
	# Initial UI state
	reset_ui()

func _on_launch_button_pressed():
	start_launch()

func _on_design_button_pressed():
	show_design_screen()

func _on_try_again_button_pressed():
	if last_launch_success:
		# Success - go back to main menu
		reset_ui()
	else:
		# Failure - go back to testing screen to fix issues
		result_panel.visible = false
		if rocket_sprite:
			rocket_sprite.reset()
		show_testing_screen()

func show_design_screen():
	# Create design screen if not exists
	if design_screen == null:
		design_screen = design_screen_scene.instantiate()
		design_screen.launch_requested.connect(_on_design_launch_requested)
		design_screen.back_requested.connect(_on_design_back_requested)
		if design_screen.has_signal("testing_requested"):
			design_screen.testing_requested.connect(_on_design_testing_requested)
		add_child(design_screen)

	# Hide main content
	main_content.visible = false
	rocket_visual.visible = false

	# Show design screen
	design_screen.visible = true

func hide_design_screen():
	if design_screen:
		design_screen.visible = false

	# Show main content
	main_content.visible = true
	rocket_visual.visible = true

func show_testing_screen():
	# Ensure design screen exists to get the designer
	if design_screen == null:
		design_screen = design_screen_scene.instantiate()
		design_screen.launch_requested.connect(_on_design_launch_requested)
		design_screen.back_requested.connect(_on_design_back_requested)
		design_screen.testing_requested.connect(_on_design_testing_requested)
		add_child(design_screen)
		design_screen.visible = false

	# Create testing screen if not exists
	if testing_screen == null:
		testing_screen = testing_screen_scene.instantiate()
		testing_screen.launch_requested.connect(_on_testing_launch_requested)
		testing_screen.back_requested.connect(_on_testing_back_requested)
		add_child(testing_screen)

	# Pass the designer to the testing screen
	var designer = design_screen.get_designer()
	testing_screen.set_designer(designer)

	# Hide main content and design screen
	main_content.visible = false
	rocket_visual.visible = false
	if design_screen:
		design_screen.visible = false

	# Show testing screen
	testing_screen.visible = true

func hide_testing_screen():
	if testing_screen:
		testing_screen.visible = false

	# Show main content
	main_content.visible = true
	rocket_visual.visible = true

func _on_design_launch_requested():
	# Go to testing screen instead of launching directly
	_on_design_testing_requested()

func _on_design_testing_requested():
	# Hide design screen and show testing screen
	if design_screen:
		design_screen.visible = false

	show_testing_screen()

func _on_design_back_requested():
	hide_design_screen()
	reset_ui()

func _on_testing_launch_requested():
	# Copy design from designer to launcher
	if design_screen:
		var designer = design_screen.get_designer()
		launcher.copy_design_from(designer)
		has_custom_design = true

	# Hide testing screen
	hide_testing_screen()

	# Start the launch
	start_launch()

func _on_testing_back_requested():
	# Go back to design screen
	hide_testing_screen()
	show_design_screen()

func start_launch():
	# Update state
	attempt_count += 1

	# Update UI
	launch_button.visible = false
	design_button.visible = false
	status_panel.visible = true
	result_panel.visible = false

	# Clear previous stage list
	for child in stage_list.get_children():
		child.queue_free()

	# Start rocket animation with correct stage count for gravity turn
	if rocket_sprite:
		var stage_count = launcher.get_stage_count()
		rocket_sprite.set_total_stages(stage_count)
		rocket_sprite.start_launch()

	# Run the launch with proper timing control
	await run_launch_with_delays()

func reset_ui():
	# Reset panels
	launch_button.visible = true
	design_button.visible = true
	status_panel.visible = false
	result_panel.visible = false

	# Reset rocket animation
	if rocket_sprite:
		rocket_sprite.reset()

	# Update subtitle with stats
	if attempt_count == 0:
		subtitle.text = "Ready for First Launch"
	else:
		var design_info = ""
		if has_custom_design and launcher.has_design():
			var design_name = launcher.get_design_name()
			if design_name != "":
				design_info = " | Design: " + design_name
		subtitle.text = "Attempts: %d | Successes: %d | Success Rate: %.1f%%%s" % [
			attempt_count,
			success_count,
			(float(success_count) / float(attempt_count)) * 100.0,
			design_info
		]

func run_launch_with_delays():
	# Get stage count and run simulation stage by stage
	var stage_count = launcher.get_stage_count()
	var success = true
	var failed_stage_name = ""

	# Go through each stage with delays
	for i in range(stage_count):
		var stage_name = launcher.get_stage_description(i)
		# Use total failure rate which includes flaw contributions
		var failure_rate = launcher.get_total_failure_rate(i)

		# Show this stage as in progress first
		var label = Label.new()
		label.text = "► " + stage_name + "..."
		label.add_theme_color_override("font_color", Color(1.0, 1.0, 0.3))
		label.add_theme_font_size_override("font_size", 16)
		stage_list.add_child(label)

		# Advance rocket animation
		if rocket_sprite:
			rocket_sprite.advance_stage()

		# Wait before checking result
		await get_tree().create_timer(0.8).timeout

		# Check if this stage fails
		var roll = randf()
		if roll < failure_rate:
			# Failed!
			label.text = "✗ " + stage_name + " - FAILED"
			label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
			success = false
			failed_stage_name = stage_name
			await get_tree().create_timer(0.2).timeout
			break
		else:
			# Passed!
			label.text = "✓ " + stage_name + " - PASSED"
			label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
			await get_tree().create_timer(0.2).timeout

	# Wait a moment for effect
	await get_tree().create_timer(0.5).timeout

	# Show result animation
	if rocket_sprite:
		if success:
			# Success effects
			rocket_sprite.show_success()
			if screen_effects:
				screen_effects.flash_success()
			await get_tree().create_timer(1.0).timeout
		else:
			# Explosion effects
			rocket_sprite.show_explosion()
			if screen_effects:
				screen_effects.flash_explosion()
			await get_tree().create_timer(1.2).timeout

	# Update state
	last_launch_success = success
	if success:
		success_count += 1

	# Show result panel
	status_panel.visible = false
	result_panel.visible = true

	# Update result display
	if success:
		result_label.text = "SUCCESS!"
		result_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
		message_label.text = "Rocket reached Low Earth Orbit!"
		try_again_button.text = "CONTINUE"
	else:
		result_label.text = "FAILURE"
		result_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
		message_label.text = "Failure during " + failed_stage_name + ". Rocket lost."
		try_again_button.text = "BACK TO TESTING"
