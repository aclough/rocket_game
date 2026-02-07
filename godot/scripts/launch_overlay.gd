extends CanvasLayer

## Launch overlay for showing launch animation
## Displays launch sequence, handles success/failure

signal launch_completed(success: bool)

var game_manager: GameManager = null
var designer: RocketDesigner = null
var last_launch_success: bool = false

# UI references
@onready var title_label = $ContentMargin/VBox/HeaderPanel/HeaderMargin/HeaderVBox/Title
@onready var mission_label = $ContentMargin/VBox/HeaderPanel/HeaderMargin/HeaderVBox/MissionLabel
@onready var stages_list = $ContentMargin/VBox/ContentCenter/ContentHBox/StagesPanel/StagesMargin/StagesVBox/StagesScroll/StagesList
@onready var result_panel = $ContentMargin/VBox/ResultPanel
@onready var result_label = $ContentMargin/VBox/ResultPanel/ResultMargin/ResultVBox/ResultLabel
@onready var message_label = $ContentMargin/VBox/ResultPanel/ResultMargin/ResultVBox/MessageLabel
@onready var continue_button = $ContentMargin/VBox/ResultPanel/ResultMargin/ResultVBox/ContinueButton

# Rocket components
@onready var rocket_sprite = $ContentMargin/VBox/ContentCenter/ContentHBox/RocketVisual/RocketSprite
@onready var launcher = $RocketLauncher
@onready var screen_effects = $ScreenEffects

# Sky/space transition
@onready var background = $Background
@onready var starfield = $Starfield
const SKY_COLOR = Color(0.4, 0.6, 0.9, 1.0)  # Blue sky
const SPACE_COLOR = Color(0.02, 0.02, 0.05, 1.0)  # Dark space

func _ready():
	visible = false

func _set_altitude(progress: float):
	# Transition from sky to space as rocket ascends
	# progress: 0 = on pad, 1 = in orbit
	var clamped = clamp(progress, 0.0, 1.0)

	if background:
		background.color = SKY_COLOR.lerp(SPACE_COLOR, clamped)

	if starfield:
		starfield.set_altitude(clamped)

func set_game_manager(gm: GameManager):
	game_manager = gm

func set_designer(d: RocketDesigner):
	designer = d

func show_launch(gm: GameManager, d: RocketDesigner):
	game_manager = gm
	designer = d

	# Copy design to launcher
	launcher.copy_design_from(designer)

	# Update header
	if game_manager.has_active_contract():
		mission_label.text = "Mission: " + game_manager.get_active_contract_name()
	else:
		mission_label.text = "Free Launch"

	# Reset UI
	title_label.text = "LAUNCH IN PROGRESS"
	result_panel.visible = false

	# Clear stages list
	for child in stages_list.get_children():
		child.queue_free()

	# Set up rocket sprite
	var stage_count = launcher.get_stage_count()
	rocket_sprite.set_total_stages(stage_count)
	rocket_sprite.reset()

	# Reset sky to ground level (blue sky, no stars)
	_set_altitude(0.0)

	# Show overlay
	visible = true

	# Start the launch sequence
	await run_launch_with_delays()

func run_launch_with_delays():
	var stage_count = launcher.get_stage_count()
	var success = true
	var failed_stage_name = ""
	var discovered_flaw_name = ""

	# Start rocket animation
	rocket_sprite.start_launch()

	# Go through each stage with delays
	for i in range(stage_count):
		var stage_name = launcher.get_stage_description(i)
		var failure_rate = launcher.get_total_failure_rate(i)

		# Show this stage as in progress
		var label = Label.new()
		label.text = "> " + stage_name + "..."
		label.add_theme_color_override("font_color", Color(1.0, 1.0, 0.3))
		label.add_theme_font_size_override("font_size", 16)
		stages_list.add_child(label)

		# Advance rocket animation
		rocket_sprite.advance_stage()

		# Update sky/space transition based on progress
		var altitude_progress = float(i + 1) / float(stage_count)
		_set_altitude(altitude_progress)

		# Wait before checking result
		await get_tree().create_timer(0.8).timeout

		# Check if this stage fails
		var roll = randf()
		if roll < failure_rate:
			# Failed!
			label.text = "X " + stage_name + " - FAILED"
			label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
			success = false
			failed_stage_name = stage_name

			# Determine if a flaw caused this failure
			if designer:
				var base_rate = launcher.get_stage_failure_rate(i)
				var flaw_rate = launcher.get_flaw_failure_rate(i)

				if flaw_rate > 0:
					var flaw_probability = flaw_rate / failure_rate
					var flaw_roll = randf()
					if flaw_roll < flaw_probability:
						var rocket_stage = launcher.get_event_rocket_stage(i)
						var stage_engine_type = -1
						if rocket_stage >= 0:
							stage_engine_type = designer.get_stage_engine_type(rocket_stage)
						var flaw_id = designer.check_flaw_trigger(stage_name, stage_engine_type)
						if flaw_id >= 0:
							discovered_flaw_name = designer.discover_flaw_by_id(flaw_id)
							# Also discover in game state's engine registry so teams can fix it
							if game_manager:
								game_manager.discover_engine_flaw_by_id(flaw_id)

			await get_tree().create_timer(0.2).timeout
			break
		else:
			# Passed!
			label.text = "* " + stage_name + " - PASSED"
			label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
			await get_tree().create_timer(0.2).timeout

	# Wait a moment for effect
	await get_tree().create_timer(0.5).timeout

	# Show result animation
	if success:
		rocket_sprite.show_success()
		if screen_effects:
			screen_effects.flash_success()
		await get_tree().create_timer(1.0).timeout
	else:
		rocket_sprite.show_explosion()
		if screen_effects:
			screen_effects.flash_explosion()
		await get_tree().create_timer(1.2).timeout

	# Update state
	last_launch_success = success

	# Handle contract completion/failure
	var reward = 0.0
	var destination = ""
	if game_manager and game_manager.has_active_contract():
		destination = game_manager.get_active_contract_destination()
		if success:
			reward = game_manager.complete_contract()
		else:
			game_manager.fail_contract()
		game_manager.update_current_rocket_design()

	# Show result panel
	result_panel.visible = true
	title_label.text = "LAUNCH COMPLETE"

	if success:
		result_label.text = "SUCCESS!"
		result_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
		if reward > 0:
			message_label.text = "Rocket reached %s!\nReward: %s\nNew Balance: %s" % [
				destination,
				_format_money(reward),
				game_manager.get_money_formatted()
			]
		else:
			message_label.text = "Rocket reached orbit!"
		continue_button.text = "CONTINUE"
	else:
		result_label.text = "FAILURE"
		result_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
		if discovered_flaw_name != "":
			message_label.text = "Failure during %s.\nCause identified: %s\nThis issue has been added to your known problems." % [failed_stage_name, discovered_flaw_name]
		else:
			message_label.text = "Failure during %s. Rocket lost." % failed_stage_name
		continue_button.text = "BACK TO TESTING"

func _on_continue_pressed():
	visible = false
	launch_completed.emit(last_launch_success)

func _format_money(value: float) -> String:
	if value >= 1_000_000_000:
		return "$%.1fB" % (value / 1_000_000_000)
	elif value >= 1_000_000:
		return "$%.0fM" % (value / 1_000_000)
	elif value >= 1_000:
		return "$%.0fK" % (value / 1_000)
	else:
		return "$%.0f" % value
