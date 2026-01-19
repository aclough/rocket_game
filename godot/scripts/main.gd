extends Control

# UI references
@onready var launch_button = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/LaunchButton
@onready var status_panel = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/StatusPanel
@onready var stage_list = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/StatusPanel/MarginContainer/VBox/ScrollContainer/StageList
@onready var result_panel = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/ResultPanel
@onready var result_label = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/ResultPanel/MarginContainer/VBox/ResultLabel
@onready var message_label = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/ResultPanel/MarginContainer/VBox/CenterContainer/MessageLabel
@onready var try_again_button = $MarginContainer/VBoxContainer/CenterContainer/ContentVBox/ResultPanel/MarginContainer/VBox/TryAgainButton
@onready var subtitle = $MarginContainer/VBoxContainer/HeaderContainer/Subtitle

# Rocket launcher reference
@onready var launcher = $RocketLauncher

# Rocket visual reference
@onready var rocket_sprite = $RocketVisual/RocketSprite

# Screen effects reference
@onready var screen_effects = $ScreenEffects

# State tracking
var attempt_count = 0
var success_count = 0

func _ready():
	# Initial UI state
	reset_ui()

func _on_launch_button_pressed():
	start_launch()

func _on_try_again_button_pressed():
	reset_ui()

func start_launch():
	# Update state
	attempt_count += 1

	# Update UI
	launch_button.visible = false
	status_panel.visible = true
	result_panel.visible = false

	# Clear previous stage list
	for child in stage_list.get_children():
		child.queue_free()

	# Start rocket animation
	if rocket_sprite:
		rocket_sprite.start_launch()

	# Run the launch with proper timing control
	await run_launch_with_delays()

func reset_ui():
	# Reset panels
	launch_button.visible = true
	status_panel.visible = false
	result_panel.visible = false

	# Reset rocket animation
	if rocket_sprite:
		rocket_sprite.reset()

	# Update subtitle with stats
	if attempt_count == 0:
		subtitle.text = "Ready for First Launch"
	else:
		subtitle.text = "Attempts: %d | Successes: %d | Success Rate: %.1f%%" % [
			attempt_count,
			success_count,
			(float(success_count) / float(attempt_count)) * 100.0
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

		# Show this stage
		var label = Label.new()
		label.text = "✓ " + stage_name + " - PASSED"
		label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
		label.add_theme_font_size_override("font_size", 16)
		stage_list.add_child(label)

		# Advance rocket animation
		if rocket_sprite:
			rocket_sprite.advance_stage()

		# Wait 1 second before next stage
		await get_tree().create_timer(1.0).timeout

		# Check if this stage fails
		var roll = randf()
		if roll < failure_rate:
			success = false
			failed_stage_name = stage_name
			break

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
		result_label.text = "🎉 SUCCESS! 🎉"
		result_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
		message_label.text = "Success! Rocket reached Low Earth Orbit!"
	else:
		result_label.text = "💥 FAILURE 💥"
		result_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
		message_label.text = "Failure during " + failed_stage_name + ". Rocket exploded."
