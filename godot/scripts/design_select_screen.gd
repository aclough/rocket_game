extends Control

signal design_selected(design_index: int)  # -1 for new design
signal back_requested

# Game manager reference (set by parent)
var game_manager: GameManager = null

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

	var design_count = game_manager.get_saved_design_count()

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

func _create_design_card(index: int, required_dv: float) -> PanelContainer:
	var name = game_manager.get_saved_design_name(index)
	var delta_v = game_manager.get_saved_design_delta_v(index)
	var cost = game_manager.get_saved_design_cost(index)
	var success_rate = game_manager.get_saved_design_success_rate(index) * 100
	var stages = game_manager.get_saved_design_stage_count(index)
	var has_flaws = game_manager.saved_design_has_flaws(index)
	var discovered = game_manager.get_saved_design_discovered_flaw_count(index)
	var fixed = game_manager.get_saved_design_fixed_flaw_count(index)

	var panel = PanelContainer.new()

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
	name_label.add_theme_font_size_override("font_size", 16)
	info_vbox.add_child(name_label)

	var stats_label = Label.new()
	stats_label.text = "%d stages | %.0f m/s | $%s" % [stages, delta_v, _format_money(cost)]
	stats_label.add_theme_font_size_override("font_size", 12)
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

	# Buttons
	var buttons_vbox = VBoxContainer.new()
	buttons_vbox.add_theme_constant_override("separation", 5)
	hbox.add_child(buttons_vbox)

	var select_btn = Button.new()
	select_btn.text = "SELECT"
	select_btn.custom_minimum_size = Vector2(100, 35)
	select_btn.add_theme_font_size_override("font_size", 14)
	select_btn.pressed.connect(_on_select_design_pressed.bind(index))
	# Disable selection if rocket doesn't meet delta-v requirements
	if not meets_requirements:
		select_btn.disabled = true
		select_btn.tooltip_text = "Insufficient delta-v for this mission"
	buttons_vbox.add_child(select_btn)

	var edit_btn = Button.new()
	edit_btn.text = "EDIT"
	edit_btn.custom_minimum_size = Vector2(100, 30)
	edit_btn.add_theme_font_size_override("font_size", 12)
	edit_btn.pressed.connect(_on_edit_design_pressed.bind(index))
	buttons_vbox.add_child(edit_btn)

	var delete_btn = Button.new()
	delete_btn.text = "DELETE"
	delete_btn.custom_minimum_size = Vector2(100, 25)
	delete_btn.add_theme_font_size_override("font_size", 10)
	delete_btn.pressed.connect(_on_delete_design_pressed.bind(index))
	buttons_vbox.add_child(delete_btn)

	return panel

func _on_select_design_pressed(index: int):
	if game_manager:
		game_manager.load_design(index)
	design_selected.emit(index)

func _on_edit_design_pressed(index: int):
	if game_manager:
		game_manager.load_design(index)
	# Emit with special value to indicate we want to edit
	design_selected.emit(index)

func _on_delete_design_pressed(index: int):
	if game_manager:
		game_manager.delete_saved_design(index)

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
