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

# State tracking
var attempt_count = 0
var success_count = 0
var has_custom_design = false

func _ready():
	# Initial UI state
	reset_ui()

func _on_launch_button_pressed():
	start_launch()

func _on_design_button_pressed():
	show_design_screen()

func _on_try_again_button_pressed():
	reset_ui()

func show_design_screen():
	# Create design screen if not exists
	if design_screen == null:
		design_screen = design_screen_scene.instantiate()
		design_screen.launch_requested.connect(_on_design_launch_requested)
		design_screen.back_requested.connect(_on_design_back_requested)
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

func _on_design_launch_requested():
	# Copy design from designer to launcher
	if design_screen:
		var designer = design_screen.get_designer()
		launcher.copy_design_from(designer)
		has_custom_design = true

	# Hide design screen
	hide_design_screen()

	# Start the launch
	start_launch()

func _on_design_back_requested():
	hide_design_screen()
	reset_ui()

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
		var failure_rate = launcher.get_stage_failure_rate(i)

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
	else:
		result_label.text = "FAILURE"
		result_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
		message_label.text = "Failure during " + failed_stage_name + ". Rocket lost."
