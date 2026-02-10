extends Control

signal design_selected(design_index: int)  # -1 for new design
signal back_requested
signal engine_edit_requested(engine_index: int)

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

# Cached team person icon
var _eng_team_icon: ImageTexture = null

func _create_person_icon(color: Color) -> ImageTexture:
	var img = Image.create(16, 16, false, Image.FORMAT_RGBA8)
	img.fill(Color(0, 0, 0, 0))
	# Head: filled circle at (8, 4), radius 3
	for y in range(16):
		for x in range(16):
			var dx = x - 8
			var dy = y - 4
			if dx * dx + dy * dy <= 9:
				img.set_pixel(x, y, color)
	# Body: trapezoid y=8..15, widening from ~3px to ~6px half-width
	for y in range(8, 16):
		var t = float(y - 8) / 7.0
		var half_w = int(3.0 + t * 3.0)
		for x in range(8 - half_w, 8 + half_w + 1):
			if x >= 0 and x < 16:
				img.set_pixel(x, y, color)
	var tex = ImageTexture.create_from_image(img)
	return tex

func _get_eng_team_icon() -> ImageTexture:
	if _eng_team_icon == null:
		_eng_team_icon = _create_person_icon(Color(0.4, 0.7, 1.0))
	return _eng_team_icon

func _create_team_count_icons_hbox(count: int, icon: ImageTexture) -> HBoxContainer:
	var hbox = HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 4)
	for i in range(count):
		var tex_rect = TextureRect.new()
		tex_rect.texture = icon
		tex_rect.custom_minimum_size = Vector2(16, 16)
		tex_rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		hbox.add_child(tex_rect)
	return hbox

# UI references
@onready var mission_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderHBox/TitleVBox/MissionLabel
@onready var requirements_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderHBox/TitleVBox/RequirementsLabel
@onready var designs_list = $MarginContainer/VBox/ContentPanel/ContentMargin/ContentHBox/SavedDesignsPanel/SavedMargin/SavedVBox/DesignsScroll/DesignsList

func _ready():
	pass

func _on_back_pressed():
	back_requested.emit()

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

	# --- Rocket Designs Section ---
	var rocket_header = Label.new()
	rocket_header.text = "Rocket Designs"
	rocket_header.add_theme_font_size_override("font_size", 18)
	rocket_header.add_theme_color_override("font_color", Color(0.7, 0.8, 1.0))
	designs_list.add_child(rocket_header)

	var design_count = game_manager.get_rocket_design_count()

	if design_count == 0:
		var label = Label.new()
		label.text = "No saved designs yet.\nCreate a new design to get started."
		label.add_theme_font_size_override("font_size", 14)
		label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		designs_list.add_child(label)
	else:
		# Get mission requirements for comparison
		var required_dv = 0.0
		if game_manager.has_active_contract():
			required_dv = game_manager.get_active_contract_delta_v()

		for i in range(design_count):
			var card = _create_design_card(i, required_dv)
			designs_list.add_child(card)

	# --- Separator ---
	var sep = HSeparator.new()
	sep.add_theme_constant_override("separation", 20)
	designs_list.add_child(sep)

	# --- Engine Designs Section ---
	var engine_header = Label.new()
	engine_header.text = "Engine Designs"
	engine_header.add_theme_font_size_override("font_size", 18)
	engine_header.add_theme_color_override("font_color", Color(1.0, 0.8, 0.6))
	designs_list.add_child(engine_header)

	var engine_count = game_manager.get_engine_type_count()
	for i in range(engine_count):
		var card = _create_engine_design_card(i)
		designs_list.add_child(card)

func _create_design_card(index: int, required_dv: float) -> Control:
	var design_name = game_manager.get_rocket_design_name(index)
	var delta_v = game_manager.get_rocket_design_delta_v(index)
	var cost = game_manager.get_rocket_design_cost(index)
	var mass = game_manager.get_rocket_design_mass(index)
	var testing_level = game_manager.get_rocket_design_testing_level(index)
	var testing_level_name = game_manager.get_rocket_design_testing_level_name(index)
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
	name_label.text = design_name
	name_label.add_theme_font_size_override("font_size", 20)
	info_vbox.add_child(name_label)

	var tank_material_name = game_manager.get_rocket_design_tank_material_name(index)

	var stats_label = Label.new()
	stats_label.text = "%d stages | %s | %.0f m/s | %s | $%s" % [stages, tank_material_name, delta_v, _format_mass(mass), _format_money(cost)]
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

	# Testing level
	var testing_vbox = VBoxContainer.new()
	testing_vbox.alignment = BoxContainer.ALIGNMENT_CENTER
	hbox.add_child(testing_vbox)

	var testing_label = Label.new()
	testing_label.text = testing_level_name
	testing_label.add_theme_font_size_override("font_size", 16)
	testing_label.add_theme_color_override("font_color", _testing_level_color(testing_level))
	testing_vbox.add_child(testing_label)

	var testing_title = Label.new()
	testing_title.text = "testing"
	testing_title.add_theme_font_size_override("font_size", 10)
	testing_title.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	testing_vbox.add_child(testing_title)

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
	# Disable editing if design is in Engineering/Testing phase
	if not can_edit:
		edit_btn.disabled = true
		edit_btn.tooltip_text = "Cannot edit while in Engineering/Testing phase"
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
		var status_hbox = HBoxContainer.new()
		status_hbox.add_theme_constant_override("separation", 8)
		var status_label = Label.new()
		status_label.text = "%s" % status
		status_label.add_theme_font_size_override("font_size", 11)
		if base_status == "Testing":
			status_label.add_theme_color_override("font_color", Color(0.4, 0.6, 1.0))
		elif base_status == "Fixing":
			status_label.add_theme_color_override("font_color", Color(1.0, 0.7, 0.3))
		elif base_status == "Engineering":
			status_label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
		status_hbox.add_child(status_label)
		if teams_count > 0:
			var icons = _create_team_count_icons_hbox(teams_count, _get_eng_team_icon())
			status_hbox.add_child(icons)
		info_vbox.add_child(status_hbox)

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
		elif base_status == "Testing":
			# Blue bar showing actual progress for Testing
			var progress_bar = ProgressBar.new()
			progress_bar.value = progress * 100
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

# ==========================================
# Engine Design Cards
# ==========================================

func _create_engine_design_card(index: int) -> Control:
	var engine_name = game_manager.get_engine_type_name(index)
	var fuel_type_name = game_manager.get_engine_design_fuel_type_name(index)
	var engine_scale = game_manager.get_engine_design_scale(index)
	var thrust = game_manager.get_engine_design_thrust(index)
	var ve = game_manager.get_engine_design_exhaust_velocity(index)
	var mass = game_manager.get_engine_design_mass(index)
	var cost = game_manager.get_engine_design_cost(index)
	var status = game_manager.get_engine_status(index)
	var base_status = game_manager.get_engine_status_base(index)
	var _can_modify = game_manager.can_modify_engine_design(index)
	var teams_count = game_manager.get_teams_on_engine_count(index)

	var wrapper = DesignCardWrapper.new()
	wrapper.design_index = index
	wrapper.game_manager = game_manager
	wrapper.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	wrapper.team_assigned.connect(_on_team_assigned_to_engine)

	var panel = PanelContainer.new()
	var style = StyleBoxFlat.new()
	style.set_bg_color(Color(0.12, 0.10, 0.06))
	style.set_border_width_all(1)
	style.set_border_color(Color(0.6, 0.5, 0.3, 0.5))
	panel.add_theme_stylebox_override("panel", style)
	wrapper.add_child(panel)

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 15)
	margin.add_theme_constant_override("margin_right", 15)
	margin.add_theme_constant_override("margin_top", 10)
	margin.add_theme_constant_override("margin_bottom", 10)
	panel.add_child(margin)

	var hbox = HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 15)
	margin.add_child(hbox)

	# Engine info
	var info_vbox = VBoxContainer.new()
	info_vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	info_vbox.add_theme_constant_override("separation", 3)
	hbox.add_child(info_vbox)

	var name_label = Label.new()
	name_label.text = engine_name
	name_label.add_theme_font_size_override("font_size", 20)
	info_vbox.add_child(name_label)

	var complexity = game_manager.get_engine_design_complexity(index)
	var stats_label = Label.new()
	stats_label.text = "%s | %.0fx | C%d | %.0f kN | %.0f m/s | %s | $%s" % [
		fuel_type_name, engine_scale, complexity, thrust, ve, _format_mass(mass), _format_money(cost)
	]
	stats_label.add_theme_font_size_override("font_size", 14)
	stats_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	info_vbox.add_child(stats_label)

	# Status label
	if base_status != "Untested":
		var status_hbox = HBoxContainer.new()
		status_hbox.add_theme_constant_override("separation", 8)
		var status_label = Label.new()
		status_label.text = status
		status_label.add_theme_font_size_override("font_size", 11)
		if base_status == "Testing":
			status_label.add_theme_color_override("font_color", Color(0.4, 0.6, 1.0))
		elif base_status == "Fixing":
			status_label.add_theme_color_override("font_color", Color(1.0, 0.7, 0.3))
		status_hbox.add_child(status_label)
		if teams_count > 0:
			var icons = _create_team_count_icons_hbox(teams_count, _get_eng_team_icon())
			status_hbox.add_child(icons)
		info_vbox.add_child(status_hbox)
	else:
		var untested_label = Label.new()
		untested_label.text = "Untested"
		untested_label.add_theme_font_size_override("font_size", 11)
		untested_label.add_theme_color_override("font_color", Color(1.0, 0.5, 0.3))
		info_vbox.add_child(untested_label)

	# Buttons
	var buttons_vbox = VBoxContainer.new()
	buttons_vbox.add_theme_constant_override("separation", 5)
	hbox.add_child(buttons_vbox)

	var edit_btn = Button.new()
	edit_btn.text = "EDIT"
	edit_btn.custom_minimum_size = Vector2(100, 35)
	edit_btn.add_theme_font_size_override("font_size", 14)
	edit_btn.pressed.connect(_on_edit_engine_pressed.bind(index))
	buttons_vbox.add_child(edit_btn)

	var dup_btn = Button.new()
	dup_btn.text = "DUPLICATE"
	dup_btn.custom_minimum_size = Vector2(100, 28)
	dup_btn.add_theme_font_size_override("font_size", 11)
	dup_btn.pressed.connect(_on_duplicate_engine_pressed.bind(index))
	buttons_vbox.add_child(dup_btn)

	var del_btn = Button.new()
	del_btn.text = "DELETE"
	del_btn.custom_minimum_size = Vector2(100, 25)
	del_btn.add_theme_font_size_override("font_size", 10)
	del_btn.pressed.connect(_on_delete_engine_pressed.bind(index))
	buttons_vbox.add_child(del_btn)

	return wrapper

func _on_team_assigned_to_engine(design_index: int, team_id: int):
	if game_manager:
		game_manager.assign_team_to_engine(team_id, design_index)
		_update_ui()

func _on_edit_engine_pressed(index: int):
	engine_edit_requested.emit(index)

func _on_duplicate_engine_pressed(index: int):
	if game_manager:
		var new_idx = game_manager.duplicate_engine_design(index)
		if new_idx >= 0:
			engine_edit_requested.emit(new_idx)

func _on_delete_engine_pressed(index: int):
	if game_manager:
		game_manager.delete_engine_design(index)

func _on_new_engine_pressed():
	engine_edit_requested.emit(-1)

# Helper to get color for a testing level index (0-4)
func _testing_level_color(level: int) -> Color:
	match level:
		0: return Color(1.0, 0.3, 0.3)       # Untested - Red
		1: return Color(1.0, 0.6, 0.2)       # Lightly Tested - Orange
		2: return Color(1.0, 1.0, 0.3)       # Moderately Tested - Yellow
		3: return Color(0.6, 1.0, 0.4)       # Well Tested - Light green
		4: return Color(0.3, 1.0, 0.3)       # Thoroughly Tested - Green
		_: return Color(0.5, 0.5, 0.5)

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
