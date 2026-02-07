extends Control

signal design_selected(design_index: int)  # -1 for new design
signal back_requested

# Game manager reference (set by parent)
var game_manager: GameManager = null

## Helper class for design cards that can receive team drops
## Extends MarginContainer to properly propagate child sizing
class DesignCardWrapper extends MarginContainer:
	var design_index: int = -1
	var game_manager: GameManager = null
	signal team_assigned(design_index: int, team_id: int)

	func _ready():
		mouse_filter = Control.MOUSE_FILTER_PASS

	func _can_drop_data(_at_position: Vector2, data) -> bool:
		if data is Dictionary and data.get("type") == "team":
			return true
		return false

	func _drop_data(_at_position: Vector2, data) -> void:
		if data is Dictionary and data.get("type") == "team":
			var team_id = data.get("team_id", -1)
			if team_id >= 0:
				team_assigned.emit(design_index, team_id)

# Delete confirmation
var pending_delete_index: int = -1
var confirm_dialog: ConfirmationDialog = null

# UI references
@onready var mission_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderHBox/TitleVBox/MissionLabel
@onready var requirements_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderHBox/TitleVBox/RequirementsLabel
@onready var designs_list = $MarginContainer/VBox/ContentPanel/ContentMargin/ContentHBox/SavedDesignsPanel/SavedMargin/SavedVBox/DesignsScroll/DesignsList

func _ready():
	pass

func set_game_manager(gm: GameManager):
	game_manager = gm
	if game_manager:
		# Use string-based signal connection for GDExtension compatibility
		if not game_manager.is_connected("designs_changed", _on_designs_changed):
			game_manager.connect("designs_changed", _on_designs_changed)
		_update_ui()

func _on_designs_changed():
	_update_ui()

func _update_ui():
	if not game_manager:
		return

	# Update header with mission info
	if game_manager.has_active_contract():
		mission_label.text = "Mission: " + game_manager.get_active_contract_name()
		var delta_v = game_manager.get_active_contract_delta_v()
		var payload = game_manager.get_active_contract_payload()
		requirements_label.text = "Requirements: %.0f m/s | %.0f kg payload" % [delta_v, payload]
	else:
		mission_label.text = "Free Launch Mode"
		requirements_label.text = "No specific requirements"

	# Rebuild designs list
	_rebuild_designs_list()

func _rebuild_designs_list():
	# Clear existing
	for child in designs_list.get_children():
		child.queue_free()

	if not game_manager:
		return

	var design_count = game_manager.get_rocket_design_count()

	if design_count == 0:
		var label = Label.new()
		label.text = "No saved designs yet.\nCreate a new design to get started."
		label.add_theme_font_size_override("font_size", 14)
		label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		designs_list.add_child(label)
		return

	# Get mission requirements for comparison
	var required_dv = 0.0
	if game_manager.has_active_contract():
		required_dv = game_manager.get_active_contract_delta_v()

	for i in range(design_count):
		var card = _create_design_card(i, required_dv)
		designs_list.add_child(card)

func _create_design_card(index: int, required_dv: float) -> Control:
	var name = game_manager.get_rocket_design_name(index)
	var delta_v = game_manager.get_rocket_design_delta_v(index)
	var cost = game_manager.get_rocket_design_cost(index)
	var mass = game_manager.get_rocket_design_mass(index)
	var success_rate = game_manager.get_rocket_design_success_rate(index) * 100
	var stages = game_manager.get_rocket_design_stage_count(index)
	var has_flaws = game_manager.rocket_design_has_flaws(index)
	var discovered = game_manager.get_rocket_design_discovered_flaw_count(index)
	var fixed = game_manager.get_rocket_design_fixed_flaw_count(index)
	var status = game_manager.get_design_status(index)
	var base_status = game_manager.get_design_status_base(index)
	var progress = game_manager.get_design_progress(index)
	var teams_count = game_manager.get_teams_on_design_count(index)

	# Use a MarginContainer wrapper to handle drag-drop and proper sizing
	var wrapper = DesignCardWrapper.new()
	wrapper.design_index = index
	wrapper.game_manager = game_manager
	wrapper.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	wrapper.team_assigned.connect(_on_team_assigned_to_design)

	var panel = PanelContainer.new()
	wrapper.add_child(panel)

	# Color-code based on whether it meets requirements
	var meets_requirements = required_dv <= 0 or delta_v >= required_dv
	if meets_requirements:
		var style = StyleBoxFlat.new()
		style.set_bg_color(Color(0.08, 0.12, 0.08))
		style.set_border_width_all(1)
		style.set_border_color(Color(0.3, 0.6, 0.3, 0.5))
		panel.add_theme_stylebox_override("panel", style)

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 15)
	margin.add_theme_constant_override("margin_right", 15)
	margin.add_theme_constant_override("margin_top", 10)
	margin.add_theme_constant_override("margin_bottom", 10)
	panel.add_child(margin)

	var hbox = HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 15)
	margin.add_child(hbox)

	# Design info
	var info_vbox = VBoxContainer.new()
	info_vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	info_vbox.add_theme_constant_override("separation", 3)
	hbox.add_child(info_vbox)

	var name_label = Label.new()
	name_label.text = name
	name_label.add_theme_font_size_override("font_size", 20)
	info_vbox.add_child(name_label)

	var stats_label = Label.new()
	stats_label.text = "%d stages | %.0f m/s | %s | $%s" % [stages, delta_v, _format_mass(mass), _format_money(cost)]
	stats_label.add_theme_font_size_override("font_size", 16)
	stats_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	info_vbox.add_child(stats_label)

	# Show testing status if flaws have been generated
	if has_flaws:
		var flaw_label = Label.new()
		if discovered > 0:
			flaw_label.text = "Tested: %d issues found, %d fixed" % [discovered, fixed]
			if fixed >= discovered:
				flaw_label.add_theme_color_override("font_color", Color(0.3, 0.8, 0.3))
			else:
				flaw_label.add_theme_color_override("font_color", Color(1.0, 0.8, 0.3))
		else:
			flaw_label.text = "Tested: No issues found yet"
			flaw_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		flaw_label.add_theme_font_size_override("font_size", 11)
		info_vbox.add_child(flaw_label)
	else:
		var untested_label = Label.new()
		untested_label.text = "Untested"
		untested_label.add_theme_font_size_override("font_size", 11)
		untested_label.add_theme_color_override("font_color", Color(1.0, 0.5, 0.3))
		info_vbox.add_child(untested_label)

	# Success rate
	var success_vbox = VBoxContainer.new()
	success_vbox.alignment = BoxContainer.ALIGNMENT_CENTER
	hbox.add_child(success_vbox)

	var success_label = Label.new()
	success_label.text = "%.0f%%" % success_rate
	success_label.add_theme_font_size_override("font_size", 20)
	if success_rate >= 70:
		success_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
	elif success_rate >= 40:
		success_label.add_theme_color_override("font_color", Color(1.0, 1.0, 0.3))
	else:
		success_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
	success_vbox.add_child(success_label)

	var success_title = Label.new()
	success_title.text = "success"
	success_title.add_theme_font_size_override("font_size", 10)
	success_title.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	success_vbox.add_child(success_title)

	# Get edit/launch status before building buttons
	var can_edit = game_manager.can_edit_design(index)
	var can_launch = game_manager.can_launch_design(index)

	# Buttons
	var buttons_vbox = VBoxContainer.new()
	buttons_vbox.add_theme_constant_override("separation", 5)
	hbox.add_child(buttons_vbox)

	var select_btn = Button.new()
	select_btn.text = "SELECT"
	select_btn.custom_minimum_size = Vector2(100, 35)
	select_btn.add_theme_font_size_override("font_size", 14)
	select_btn.pressed.connect(_on_select_design_pressed.bind(index))
	# Disable selection if rocket doesn't meet delta-v requirements or is not ready
	if not meets_requirements:
		select_btn.disabled = true
		select_btn.tooltip_text = "Insufficient delta-v for this mission"
	elif not can_launch and base_status != "Specification" and base_status != "":
		# Design is in Engineering phase - not ready for launch
		select_btn.disabled = true
		select_btn.tooltip_text = "Design must complete Engineering before launch"
	buttons_vbox.add_child(select_btn)

	var edit_btn = Button.new()
	edit_btn.text = "EDIT"
	edit_btn.custom_minimum_size = Vector2(100, 30)
	edit_btn.add_theme_font_size_override("font_size", 12)
	edit_btn.pressed.connect(_on_edit_design_pressed.bind(index))
	# Disable editing if design is in Engineering/Refining phase
	if not can_edit:
		edit_btn.disabled = true
		edit_btn.tooltip_text = "Cannot edit while in Engineering/Refining phase"
	buttons_vbox.add_child(edit_btn)

	# Show "Submit to Engineering" for designs in Specification status
	if base_status == "Specification" or base_status == "":
		var submit_btn = Button.new()
		submit_btn.text = "SUBMIT"
		submit_btn.custom_minimum_size = Vector2(100, 28)
		submit_btn.add_theme_font_size_override("font_size", 11)
		submit_btn.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
		submit_btn.tooltip_text = "Submit to Engineering"
		submit_btn.pressed.connect(_on_submit_to_engineering_pressed.bind(index))
		buttons_vbox.add_child(submit_btn)

	var delete_btn = Button.new()
	delete_btn.text = "DELETE"
	delete_btn.custom_minimum_size = Vector2(100, 25)
	delete_btn.add_theme_font_size_override("font_size", 10)
	delete_btn.pressed.connect(_on_delete_design_pressed.bind(index))
	buttons_vbox.add_child(delete_btn)

	# Add design status info
	if base_status != "Specification" and base_status != "":
		var status_label = Label.new()
		status_label.text = "%s" % status
		if teams_count > 0:
			status_label.text += " (%d teams)" % teams_count
		status_label.add_theme_font_size_override("font_size", 11)
		if base_status == "Refining":
			status_label.add_theme_color_override("font_color", Color(0.4, 0.6, 1.0))
		elif base_status == "Fixing":
			status_label.add_theme_color_override("font_color", Color(1.0, 0.7, 0.3))
		elif base_status == "Engineering":
			status_label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
		info_vbox.add_child(status_label)

		# Progress bar for work phases
		if base_status == "Engineering" and progress > 0 and progress < 1:
			var progress_bar = ProgressBar.new()
			progress_bar.value = progress * 100
			progress_bar.custom_minimum_size = Vector2(0, 8)
			progress_bar.show_percentage = false
			info_vbox.add_child(progress_bar)
		elif base_status == "Fixing":
			var progress_bar = ProgressBar.new()
			progress_bar.value = progress * 100
			progress_bar.custom_minimum_size = Vector2(0, 8)
			progress_bar.show_percentage = false
			var fill_style = StyleBoxFlat.new()
			fill_style.set_bg_color(Color(0.9, 0.6, 0.3))
			fill_style.set_corner_radius_all(2)
			progress_bar.add_theme_stylebox_override("fill", fill_style)
			info_vbox.add_child(progress_bar)
		elif base_status == "Refining":
			# Blue bar at 100% for Refining
			var progress_bar = ProgressBar.new()
			progress_bar.value = 100
			progress_bar.custom_minimum_size = Vector2(0, 8)
			progress_bar.show_percentage = false
			var fill_style = StyleBoxFlat.new()
			fill_style.set_bg_color(Color(0.3, 0.5, 0.9))
			fill_style.set_corner_radius_all(2)
			progress_bar.add_theme_stylebox_override("fill", fill_style)
			info_vbox.add_child(progress_bar)

	return wrapper

func _on_team_assigned_to_design(design_index: int, team_id: int):
	if game_manager:
		game_manager.assign_team_to_design(team_id, design_index)
		_update_ui()

func _on_submit_to_engineering_pressed(index: int):
	if game_manager:
		game_manager.submit_design_to_engineering(index)
		_update_ui()

func _on_select_design_pressed(index: int):
	if game_manager:
		game_manager.load_rocket_design(index)
	design_selected.emit(index)

func _on_edit_design_pressed(index: int):
	if game_manager:
		game_manager.load_rocket_design(index)
	# Emit with special value to indicate we want to edit
	design_selected.emit(index)

func _on_delete_design_pressed(index: int):
	if not game_manager:
		return

	pending_delete_index = index
	var design_name = game_manager.get_rocket_design_name(index)

	# Create confirmation dialog if needed
	if not confirm_dialog:
		confirm_dialog = ConfirmationDialog.new()
		confirm_dialog.confirmed.connect(_on_delete_confirmed)
		confirm_dialog.canceled.connect(_on_delete_canceled)
		add_child(confirm_dialog)

	confirm_dialog.title = "Delete Design"
	confirm_dialog.dialog_text = "Are you sure you want to delete \"%s\"?\nThis cannot be undone." % design_name
	confirm_dialog.popup_centered()

func _on_delete_confirmed():
	if game_manager and pending_delete_index >= 0:
		game_manager.delete_rocket_design(pending_delete_index)
	pending_delete_index = -1

func _on_delete_canceled():
	pending_delete_index = -1

func _on_new_default_pressed():
	if game_manager:
		game_manager.create_default_design()
	design_selected.emit(-1)  # -1 indicates new design

func _on_new_empty_pressed():
	if game_manager:
		game_manager.create_new_design()
	design_selected.emit(-1)

func _on_back_pressed():
	back_requested.emit()

# Helper to format money values
func _format_money(value: float) -> String:
	if value >= 1_000_000_000:
		return "%.1fB" % (value / 1_000_000_000)
	elif value >= 1_000_000:
		return "%.0fM" % (value / 1_000_000)
	elif value >= 1_000:
		return "%.0fK" % (value / 1_000)
	else:
		return "%.0f" % value

# Helper to format mass values
func _format_mass(kg: float) -> String:
	if kg >= 1_000_000:
		return "%.1f kt" % (kg / 1_000_000)
	elif kg >= 1_000:
		return "%.1f t" % (kg / 1_000)
	else:
		return "%.0f kg" % kg
