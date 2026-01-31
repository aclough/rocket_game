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

# Main content container (to hide when showing other screens)
@onready var main_content = $MarginContainer
@onready var rocket_visual = $RocketVisual

# Rocket launcher reference
@onready var launcher = $RocketLauncher

# Rocket visual reference
@onready var rocket_sprite = $RocketVisual/RocketSprite

# Screen effects reference
@onready var screen_effects = $ScreenEffects

# Contract screen (loaded dynamically)
var contract_screen: Control = null
var contract_screen_scene = preload("res://scenes/contract_screen.tscn")

# Design screen (loaded dynamically)
var design_screen: Control = null
var design_screen_scene = preload("res://scenes/design_screen.tscn")

# Design selection screen (loaded dynamically)
var design_select_screen: Control = null
var design_select_screen_scene = preload("res://scenes/design_select_screen.tscn")

# Testing screen (loaded dynamically)
var testing_screen: Control = null
var testing_screen_scene = preload("res://scenes/testing_screen.tscn")

# Game manager reference (from contract screen)
var game_manager: GameManager = null

# State tracking
var attempt_count = 0
var success_count = 0
var has_custom_design = false
var last_launch_success = false
var free_launch_mode = false
var skipped_designer = false  # True if user went directly from design selection to testing

func _ready():
	# Hide main content initially - show contract screen instead
	main_content.visible = false
	rocket_visual.visible = false

	# Show contract screen
	show_contract_screen()

func _on_launch_button_pressed():
	start_launch()

func _on_design_button_pressed():
	show_design_screen()

func _on_try_again_button_pressed():
	if last_launch_success:
		# Success - go back to contract selection
		if free_launch_mode:
			reset_ui()
		else:
			show_contract_screen()
	else:
		# Failure - go back to testing screen to fix issues
		result_panel.visible = false
		if rocket_sprite:
			rocket_sprite.reset()
		show_testing_screen()

func show_contract_screen():
	# Create contract screen if not exists
	if contract_screen == null:
		contract_screen = contract_screen_scene.instantiate()
		contract_screen.contract_selected.connect(_on_contract_selected)
		contract_screen.free_launch_requested.connect(_on_free_launch_requested)
		contract_screen.new_game_requested.connect(_on_new_game_requested)
		add_child(contract_screen)
		game_manager = contract_screen.get_game_manager()

	# Hide other screens
	main_content.visible = false
	rocket_visual.visible = false
	if design_screen:
		design_screen.visible = false
	if design_select_screen:
		design_select_screen.visible = false
	if testing_screen:
		testing_screen.visible = false

	# Show contract screen
	contract_screen.visible = true

func hide_contract_screen():
	if contract_screen:
		contract_screen.visible = false

func show_design_screen():
	# Create design screen if not exists
	if design_screen == null:
		design_screen = design_screen_scene.instantiate()
		design_screen.launch_requested.connect(_on_design_launch_requested)
		design_screen.back_requested.connect(_on_design_back_requested)
		if design_screen.has_signal("testing_requested"):
			design_screen.testing_requested.connect(_on_design_testing_requested)
		add_child(design_screen)

	# Pass game manager to design screen
	design_screen.set_game_manager(game_manager)

	# Sync design from game state to designer
	var designer = design_screen.get_designer()
	if game_manager and designer:
		game_manager.sync_design_to(designer)

	# Update design screen with contract info if we have an active contract
	if game_manager and game_manager.has_active_contract():
		if designer:
			# Set target delta-v and payload from contract
			var target_dv = game_manager.get_active_contract_delta_v()
			var payload = game_manager.get_active_contract_payload()
			designer.set_target_delta_v(target_dv)
			designer.set_payload_mass(payload)

	# Hide other screens
	main_content.visible = false
	rocket_visual.visible = false
	hide_contract_screen()
	hide_design_select_screen()

	# Show design screen
	design_screen.visible = true

func hide_design_screen():
	if design_screen:
		design_screen.visible = false

func show_design_select_screen():
	# Create design select screen if not exists
	if design_select_screen == null:
		design_select_screen = design_select_screen_scene.instantiate()
		design_select_screen.design_selected.connect(_on_design_selected)
		design_select_screen.back_requested.connect(_on_design_select_back_requested)
		add_child(design_select_screen)

	# Pass game manager to design select screen
	design_select_screen.set_game_manager(game_manager)

	# Hide other screens
	main_content.visible = false
	rocket_visual.visible = false
	hide_contract_screen()
	hide_design_screen()
	hide_testing_screen()

	# Show design select screen
	design_select_screen.visible = true

func hide_design_select_screen():
	if design_select_screen:
		design_select_screen.visible = false

func show_testing_screen():
	# Ensure design screen exists to get the designer
	if design_screen == null:
		design_screen = design_screen_scene.instantiate()
		design_screen.launch_requested.connect(_on_design_launch_requested)
		design_screen.back_requested.connect(_on_design_back_requested)
		design_screen.testing_requested.connect(_on_design_testing_requested)
		add_child(design_screen)
		design_screen.visible = false

	# Sync design from game state to designer (important when skipping designer screen)
	var designer = design_screen.get_designer()
	if game_manager and designer:
		game_manager.sync_design_to(designer)

	# Create testing screen if not exists
	if testing_screen == null:
		testing_screen = testing_screen_scene.instantiate()
		testing_screen.launch_requested.connect(_on_testing_launch_requested)
		testing_screen.back_requested.connect(_on_testing_back_requested)
		add_child(testing_screen)

	# Pass the designer to the testing screen
	testing_screen.set_designer(designer)

	# Hide other screens
	main_content.visible = false
	rocket_visual.visible = false
	hide_contract_screen()
	hide_design_select_screen()
	if design_screen:
		design_screen.visible = false

	# Show testing screen
	testing_screen.visible = true

func hide_testing_screen():
	if testing_screen:
		testing_screen.visible = false

func _on_contract_selected(contract_id: int):
	free_launch_mode = false
	hide_contract_screen()
	show_design_select_screen()

func _on_free_launch_requested():
	free_launch_mode = true
	hide_contract_screen()
	show_design_select_screen()

func _on_design_selected(design_index: int):
	# design_index is -1 for new design, otherwise the saved design index
	hide_design_select_screen()
	if design_index < 0:
		# New design - go to designer
		skipped_designer = false
		show_design_screen()
	else:
		# Existing design - go straight to testing
		skipped_designer = true
		show_testing_screen()

func _on_design_select_back_requested():
	hide_design_select_screen()
	if free_launch_mode:
		reset_ui()
	else:
		show_contract_screen()

func _on_new_game_requested():
	# Reset state
	attempt_count = 0
	success_count = 0
	has_custom_design = false

	# Reset design if exists
	if design_screen:
		var designer = design_screen.get_designer()
		if designer:
			designer.load_default_design()

func _on_design_launch_requested():
	# Go to testing screen instead of launching directly
	_on_design_testing_requested()

func _on_design_testing_requested():
	# Coming from designer, so back should go to designer
	skipped_designer = false
	# Hide design screen and show testing screen
	if design_screen:
		design_screen.visible = false

	show_testing_screen()

func _on_design_back_requested():
	hide_design_screen()
	show_design_select_screen()

func _on_testing_launch_requested():
	# Copy design from designer to launcher
	if design_screen:
		var designer = design_screen.get_designer()
		launcher.copy_design_from(designer)
		has_custom_design = true
		# Ensure the design is saved before launching (so flaws persist)
		if game_manager:
			game_manager.ensure_design_saved()
			game_manager.sync_design_from(designer)

	# Hide testing screen
	hide_testing_screen()

	# Show main content for launch
	main_content.visible = true
	rocket_visual.visible = true

	# Start the launch
	start_launch()

func _on_testing_back_requested():
	# Sync design changes (including flaw fixes) back to game state
	if design_screen and game_manager:
		var designer = design_screen.get_designer()
		game_manager.sync_design_from(designer)
	# Go back to where we came from
	hide_testing_screen()
	if skipped_designer:
		show_design_select_screen()
	else:
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
	# Show main content
	main_content.visible = true
	rocket_visual.visible = true

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
	var discovered_flaw_name = ""

	# Get the designer for flaw discovery
	var designer = null
	if design_screen:
		designer = design_screen.get_designer()

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

			# Determine if a flaw caused this failure
			# P(flaw caused it | failure) = flaw_rate / total_rate
			if designer:
				var base_rate = launcher.get_stage_failure_rate(i)
				var flaw_rate = launcher.get_flaw_failure_rate(i)

				# Only check for flaw if there's flaw contribution
				if flaw_rate > 0:
					var flaw_probability = flaw_rate / failure_rate
					var flaw_roll = randf()
					if flaw_roll < flaw_probability:
						# A flaw caused this failure - find which one
						# Get the engine type of the stage that failed
						var rocket_stage = launcher.get_event_rocket_stage(i)
						var stage_engine_type = -1
						if rocket_stage >= 0:
							stage_engine_type = designer.get_stage_engine_type(rocket_stage)
						var flaw_id = designer.check_flaw_trigger(stage_name, stage_engine_type)
						if flaw_id >= 0:
							discovered_flaw_name = designer.discover_flaw_by_id(flaw_id)

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

	# Handle contract completion/failure
	var reward = 0.0
	var destination = ""
	if game_manager and game_manager.has_active_contract() and not free_launch_mode:
		destination = game_manager.get_active_contract_destination()
		if success:
			reward = game_manager.complete_contract()
		else:
			game_manager.fail_contract()
		# Save updated design state (testing_spent reset) to saved design
		game_manager.update_current_saved_design()

	# Show result panel
	status_panel.visible = false
	result_panel.visible = true

	# Update result display
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

		if free_launch_mode:
			try_again_button.text = "CONTINUE"
		else:
			try_again_button.text = "NEW CONTRACT"
	else:
		result_label.text = "FAILURE"
		result_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
		if discovered_flaw_name != "":
			message_label.text = "Failure during " + failed_stage_name + ".\nCause identified: " + discovered_flaw_name + "\nThis issue has been added to your known problems."
		else:
			message_label.text = "Failure during " + failed_stage_name + ". Rocket lost."
		try_again_button.text = "BACK TO TESTING"

func _format_money(value: float) -> String:
	if value >= 1_000_000_000:
		return "$%.1fB" % (value / 1_000_000_000)
	elif value >= 1_000_000:
		return "$%.0fM" % (value / 1_000_000)
	elif value >= 1_000:
		return "$%.0fK" % (value / 1_000)
	else:
		return "$%.0f" % value
