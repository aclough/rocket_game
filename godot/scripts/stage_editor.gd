extends Control

signal back_requested

var game_manager: GameManager = null
var stage_design_index: int = -1

# When true, the stage was created as a temporary placeholder for preview.
# BACK deletes it; SAVE keeps it and prompts for a name.
var _pending_new: bool = false

# UI references (built dynamically)
var _name_label: Label
var _rename_btn: Button
var _status_label: Label
var _engine_option: OptionButton
var _engine_count_slider: HSlider
var _engine_count_label: Label
var _propellant_slider: HSlider
var _propellant_label: Label
var _booster_check: CheckBox
var _material_option: OptionButton
var _config_container: VBoxContainer
var _stats_labels: Dictionary = {}
var _footer_btn: Button

func _ready():
	_build_ui()

func set_game_manager(gm: GameManager):
	game_manager = gm

func load_stage(index: int):
	if index < 0:
		# Create mode: need at least one engine to create a stage from
		if game_manager:
			if game_manager.get_engine_type_count() > 0:
				stage_design_index = game_manager.create_stage_design(0)
			else:
				# No engines available - can't create stage
				stage_design_index = -1
		_pending_new = true
	else:
		stage_design_index = index
		_pending_new = false
	_update_footer_button()
	_refresh_ui()

func _build_ui():
	var root_margin = MarginContainer.new()
	root_margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	root_margin.add_theme_constant_override("margin_left", 30)
	root_margin.add_theme_constant_override("margin_right", 30)
	root_margin.add_theme_constant_override("margin_top", 20)
	root_margin.add_theme_constant_override("margin_bottom", 20)
	add_child(root_margin)

	var root_vbox = VBoxContainer.new()
	root_vbox.add_theme_constant_override("separation", 15)
	root_margin.add_child(root_vbox)

	# === Header ===
	var header_panel = PanelContainer.new()
	root_vbox.add_child(header_panel)

	var header_margin = MarginContainer.new()
	header_margin.add_theme_constant_override("margin_left", 15)
	header_margin.add_theme_constant_override("margin_right", 15)
	header_margin.add_theme_constant_override("margin_top", 10)
	header_margin.add_theme_constant_override("margin_bottom", 10)
	header_panel.add_child(header_margin)

	var header_vbox = VBoxContainer.new()
	header_vbox.add_theme_constant_override("separation", 5)
	header_margin.add_child(header_vbox)

	var title_hbox = HBoxContainer.new()
	title_hbox.add_theme_constant_override("separation", 10)
	header_vbox.add_child(title_hbox)

	var title = Label.new()
	title.text = "STAGE DESIGNER"
	title.add_theme_font_size_override("font_size", 22)
	title_hbox.add_child(title)

	_name_label = Label.new()
	_name_label.text = "- Stage Name"
	_name_label.add_theme_font_size_override("font_size", 22)
	_name_label.add_theme_color_override("font_color", Color(0.8, 0.8, 0.8))
	_name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title_hbox.add_child(_name_label)

	_rename_btn = Button.new()
	_rename_btn.text = "RENAME"
	_rename_btn.add_theme_font_size_override("font_size", 12)
	_rename_btn.pressed.connect(_on_rename_pressed)
	title_hbox.add_child(_rename_btn)

	_status_label = Label.new()
	_status_label.text = "Status: Specification"
	_status_label.add_theme_font_size_override("font_size", 14)
	_status_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	header_vbox.add_child(_status_label)

	# === Config Panel ===
	var config_panel = PanelContainer.new()
	var config_style = StyleBoxFlat.new()
	config_style.set_bg_color(Color(0.08, 0.08, 0.10))
	config_style.set_border_width_all(1)
	config_style.set_border_color(Color(0.3, 0.4, 0.5, 0.5))
	config_panel.add_theme_stylebox_override("panel", config_style)
	root_vbox.add_child(config_panel)

	var config_margin = MarginContainer.new()
	config_margin.add_theme_constant_override("margin_left", 15)
	config_margin.add_theme_constant_override("margin_right", 15)
	config_margin.add_theme_constant_override("margin_top", 10)
	config_margin.add_theme_constant_override("margin_bottom", 10)
	config_panel.add_child(config_margin)

	_config_container = VBoxContainer.new()
	_config_container.add_theme_constant_override("separation", 12)
	config_margin.add_child(_config_container)

	_build_config()

	# === Stats Panel ===
	var stats_panel = PanelContainer.new()
	var stats_style = StyleBoxFlat.new()
	stats_style.set_bg_color(Color(0.06, 0.08, 0.06))
	stats_style.set_border_width_all(1)
	stats_style.set_border_color(Color(0.3, 0.5, 0.3, 0.5))
	stats_panel.add_theme_stylebox_override("panel", stats_style)
	root_vbox.add_child(stats_panel)

	var stats_margin = MarginContainer.new()
	stats_margin.add_theme_constant_override("margin_left", 15)
	stats_margin.add_theme_constant_override("margin_right", 15)
	stats_margin.add_theme_constant_override("margin_top", 10)
	stats_margin.add_theme_constant_override("margin_bottom", 10)
	stats_panel.add_child(stats_margin)

	var stats_vbox = VBoxContainer.new()
	stats_vbox.add_theme_constant_override("separation", 5)
	stats_margin.add_child(stats_vbox)

	var stats_header = Label.new()
	stats_header.text = "Computed Stats"
	stats_header.add_theme_font_size_override("font_size", 16)
	stats_header.add_theme_color_override("font_color", Color(0.6, 0.9, 0.6))
	stats_vbox.add_child(stats_header)

	var stat_names = ["Thrust", "Dry Mass", "Wet Mass", "Delta-V (no payload)", "Engine Cost", "Tank Cost", "Total Cost"]
	for stat_name in stat_names:
		var stat_label = Label.new()
		stat_label.text = "%s: --" % stat_name
		stat_label.add_theme_font_size_override("font_size", 14)
		stat_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		stats_vbox.add_child(stat_label)
		_stats_labels[stat_name] = stat_label

	# === Spacer ===
	var spacer = Control.new()
	spacer.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root_vbox.add_child(spacer)

	# === Footer Buttons ===
	var footer_hbox = HBoxContainer.new()
	footer_hbox.add_theme_constant_override("separation", 15)
	root_vbox.add_child(footer_hbox)

	var back_btn = Button.new()
	back_btn.text = "BACK"
	back_btn.custom_minimum_size = Vector2(120, 40)
	back_btn.add_theme_font_size_override("font_size", 16)
	back_btn.pressed.connect(_on_back_pressed)
	footer_hbox.add_child(back_btn)

	var footer_spacer = Control.new()
	footer_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	footer_hbox.add_child(footer_spacer)

	_footer_btn = Button.new()
	_footer_btn.text = "SAVE"
	_footer_btn.custom_minimum_size = Vector2(140, 40)
	_footer_btn.add_theme_font_size_override("font_size", 16)
	_footer_btn.pressed.connect(_on_footer_btn_pressed)
	footer_hbox.add_child(_footer_btn)

func _update_footer_button():
	if _footer_btn:
		if _pending_new:
			_footer_btn.text = "SAVE"
		else:
			_footer_btn.text = "CREATE NEW"
	if _rename_btn:
		_rename_btn.visible = not _pending_new

func _build_config():
	for child in _config_container.get_children():
		child.queue_free()

	# Engine selector
	var engine_hbox = HBoxContainer.new()
	engine_hbox.add_theme_constant_override("separation", 10)
	_config_container.add_child(engine_hbox)

	var engine_label = Label.new()
	engine_label.text = "Engine:"
	engine_label.add_theme_font_size_override("font_size", 14)
	engine_label.custom_minimum_size = Vector2(120, 0)
	engine_hbox.add_child(engine_label)

	_engine_option = OptionButton.new()
	_engine_option.custom_minimum_size = Vector2(250, 0)
	_engine_option.add_theme_font_size_override("font_size", 14)
	_engine_option.item_selected.connect(_on_engine_selected)
	engine_hbox.add_child(_engine_option)

	# Engine count
	var count_hbox = HBoxContainer.new()
	count_hbox.add_theme_constant_override("separation", 10)
	_config_container.add_child(count_hbox)

	var count_label = Label.new()
	count_label.text = "Engine Count:"
	count_label.add_theme_font_size_override("font_size", 14)
	count_label.custom_minimum_size = Vector2(120, 0)
	count_hbox.add_child(count_label)

	_engine_count_slider = HSlider.new()
	_engine_count_slider.min_value = 1
	_engine_count_slider.max_value = 12
	_engine_count_slider.step = 1
	_engine_count_slider.value = 1
	_engine_count_slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_engine_count_slider.custom_minimum_size = Vector2(200, 0)
	_engine_count_slider.value_changed.connect(_on_engine_count_changed)
	count_hbox.add_child(_engine_count_slider)

	_engine_count_label = Label.new()
	_engine_count_label.text = "1"
	_engine_count_label.add_theme_font_size_override("font_size", 14)
	_engine_count_label.custom_minimum_size = Vector2(30, 0)
	count_hbox.add_child(_engine_count_label)

	# Propellant mass
	var prop_hbox = HBoxContainer.new()
	prop_hbox.add_theme_constant_override("separation", 10)
	_config_container.add_child(prop_hbox)

	var prop_label = Label.new()
	prop_label.text = "Propellant:"
	prop_label.add_theme_font_size_override("font_size", 14)
	prop_label.custom_minimum_size = Vector2(120, 0)
	prop_hbox.add_child(prop_label)

	_propellant_slider = HSlider.new()
	_propellant_slider.min_value = 100
	_propellant_slider.max_value = 500000
	_propellant_slider.step = 100
	_propellant_slider.value = 1000
	_propellant_slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_propellant_slider.custom_minimum_size = Vector2(200, 0)
	_propellant_slider.value_changed.connect(_on_propellant_changed)
	prop_hbox.add_child(_propellant_slider)

	_propellant_label = Label.new()
	_propellant_label.text = "1000 kg"
	_propellant_label.add_theme_font_size_override("font_size", 14)
	_propellant_label.custom_minimum_size = Vector2(100, 0)
	prop_hbox.add_child(_propellant_label)

	# Tank material
	var mat_hbox = HBoxContainer.new()
	mat_hbox.add_theme_constant_override("separation", 10)
	_config_container.add_child(mat_hbox)

	var mat_label = Label.new()
	mat_label.text = "Tank Material:"
	mat_label.add_theme_font_size_override("font_size", 14)
	mat_label.custom_minimum_size = Vector2(120, 0)
	mat_hbox.add_child(mat_label)

	_material_option = OptionButton.new()
	_material_option.custom_minimum_size = Vector2(200, 0)
	_material_option.add_theme_font_size_override("font_size", 14)
	_material_option.add_item("Aluminium", 0)
	_material_option.add_item("Carbon Composite", 1)
	_material_option.item_selected.connect(_on_material_selected)
	mat_hbox.add_child(_material_option)

	# Booster checkbox
	var booster_hbox = HBoxContainer.new()
	booster_hbox.add_theme_constant_override("separation", 10)
	_config_container.add_child(booster_hbox)

	var booster_label = Label.new()
	booster_label.text = "Role:"
	booster_label.add_theme_font_size_override("font_size", 14)
	booster_label.custom_minimum_size = Vector2(120, 0)
	booster_hbox.add_child(booster_label)

	_booster_check = CheckBox.new()
	_booster_check.text = "Booster (strap-on)"
	_booster_check.add_theme_font_size_override("font_size", 14)
	_booster_check.toggled.connect(_on_booster_toggled)
	booster_hbox.add_child(_booster_check)

func _refresh_ui():
	if not game_manager or stage_design_index < 0:
		return

	var stage_name = game_manager.get_stage_design_name(stage_design_index)
	var status = game_manager.get_stage_design_status(stage_design_index)
	var base_status = game_manager.get_stage_design_status_base(stage_design_index)
	var can_modify = game_manager.can_modify_stage_design(stage_design_index)

	# Header
	if _pending_new:
		_name_label.text = "- New Stage (unsaved)"
		_status_label.text = "Configure and click SAVE"
		_status_label.add_theme_color_override("font_color", Color(0.8, 0.8, 0.4))
	else:
		_name_label.text = "- " + stage_name
		_status_label.text = "Status: " + status
		if base_status == "Specification":
			_status_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		elif base_status == "Engineering":
			_status_label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
		elif base_status == "Testing":
			_status_label.add_theme_color_override("font_color", Color(0.4, 0.6, 1.0))
		elif base_status == "Fixing":
			_status_label.add_theme_color_override("font_color", Color(1.0, 0.7, 0.3))

	# Engine selector
	var current_engine_idx = game_manager.get_stage_design_engine_index(stage_design_index)
	_engine_option.clear()
	var engine_count = game_manager.get_engine_type_count()
	var selected_option = 0
	for i in range(engine_count):
		var ename = game_manager.get_engine_type_name(i)
		_engine_option.add_item(ename, i)
		if i == current_engine_idx:
			selected_option = i
	_engine_option.selected = selected_option
	_engine_option.disabled = not can_modify

	# Engine count
	var ec = game_manager.get_stage_design_engine_count(stage_design_index)
	_engine_count_slider.set_value_no_signal(ec)
	_engine_count_label.text = "%d" % ec
	_engine_count_slider.editable = can_modify

	# Propellant mass
	var prop = game_manager.get_stage_design_propellant(stage_design_index)
	_propellant_slider.set_value_no_signal(prop)
	_propellant_label.text = _format_mass(prop)
	_propellant_slider.editable = can_modify

	# Tank material
	var mat = game_manager.get_stage_design_tank_material(stage_design_index)
	_material_option.selected = mat
	_material_option.disabled = not can_modify

	# Booster
	var is_booster = game_manager.get_stage_design_is_booster(stage_design_index)
	_booster_check.set_pressed_no_signal(is_booster)
	_booster_check.disabled = not can_modify

	_update_stats()

func _update_stats():
	if not game_manager or stage_design_index < 0:
		return

	var thrust = game_manager.get_stage_design_thrust(stage_design_index)
	var dry_mass = game_manager.get_stage_design_dry_mass(stage_design_index)
	var wet_mass = game_manager.get_stage_design_wet_mass(stage_design_index)
	var dv = game_manager.get_stage_design_delta_v(stage_design_index, 0.0)
	var cost = game_manager.get_stage_design_cost(stage_design_index)

	_stats_labels["Thrust"].text = "Thrust: %.0f kN" % thrust
	_stats_labels["Dry Mass"].text = "Dry Mass: %s" % _format_mass(dry_mass)
	_stats_labels["Wet Mass"].text = "Wet Mass: %s" % _format_mass(wet_mass)
	_stats_labels["Delta-V (no payload)"].text = "Delta-V (no payload): %.0f m/s" % dv

	# Engine cost: engine_count * per-engine cost
	var engine_idx = game_manager.get_stage_design_engine_index(stage_design_index)
	var engine_cost_per = 0.0
	if engine_idx >= 0:
		engine_cost_per = game_manager.get_engine_design_cost(engine_idx)
	var ec = game_manager.get_stage_design_engine_count(stage_design_index)
	_stats_labels["Engine Cost"].text = "Engine Cost: $%s (%d x $%s)" % [_format_money(engine_cost_per * ec), ec, _format_money(engine_cost_per)]

	var tank_cost = cost - engine_cost_per * ec
	if tank_cost < 0:
		tank_cost = 0
	_stats_labels["Tank Cost"].text = "Tank Cost: $%s" % _format_money(tank_cost)
	_stats_labels["Total Cost"].text = "Total Cost: $%s" % _format_money(cost)

# === Signal handlers ===

func _on_engine_selected(option_index: int):
	if game_manager and stage_design_index >= 0:
		game_manager.set_stage_design_engine(stage_design_index, option_index)
		_refresh_ui()

func _on_engine_count_changed(value: float):
	if game_manager and stage_design_index >= 0:
		game_manager.set_stage_design_engine_count(stage_design_index, int(value))
		_engine_count_label.text = "%d" % int(value)
		_update_stats()

func _on_propellant_changed(value: float):
	if game_manager and stage_design_index >= 0:
		game_manager.set_stage_design_propellant(stage_design_index, value)
		_propellant_label.text = _format_mass(value)
		_update_stats()

func _on_material_selected(option_index: int):
	if game_manager and stage_design_index >= 0:
		game_manager.set_stage_design_tank_material(stage_design_index, option_index)
		_update_stats()

func _on_booster_toggled(pressed: bool):
	if game_manager and stage_design_index >= 0:
		game_manager.set_stage_design_booster(stage_design_index, pressed)
		_update_stats()

func _on_rename_pressed():
	if not game_manager or stage_design_index < 0:
		return

	var dialog = ConfirmationDialog.new()
	dialog.title = "Rename Stage"

	var vbox = VBoxContainer.new()
	dialog.add_child(vbox)

	var label = Label.new()
	label.text = "Enter a new name:"
	vbox.add_child(label)

	var name_input = LineEdit.new()
	name_input.text = game_manager.get_stage_design_name(stage_design_index)
	name_input.select_all_on_focus = true
	name_input.custom_minimum_size = Vector2(300, 0)
	vbox.add_child(name_input)

	add_child(dialog)
	dialog.popup_centered()
	name_input.grab_focus()

	dialog.confirmed.connect(func():
		var new_name = name_input.text.strip_edges()
		if not new_name.is_empty():
			game_manager.rename_stage_design(stage_design_index, new_name)
			_refresh_ui()
		dialog.queue_free()
	)
	dialog.canceled.connect(func():
		dialog.queue_free()
	)

func _on_back_pressed():
	if _pending_new and game_manager and stage_design_index >= 0:
		game_manager.delete_stage_design(stage_design_index)
	_pending_new = false
	back_requested.emit()

func _on_footer_btn_pressed():
	if _pending_new:
		_on_save_new_stage()
	else:
		_on_create_variant()

func _on_save_new_stage():
	if not game_manager or stage_design_index < 0:
		return

	var dialog = ConfirmationDialog.new()
	dialog.title = "Save Stage"

	var vbox = VBoxContainer.new()
	dialog.add_child(vbox)

	var label = Label.new()
	label.text = "Enter a name for this stage:"
	vbox.add_child(label)

	var name_input = LineEdit.new()
	name_input.text = ""
	name_input.placeholder_text = "Stage name..."
	name_input.select_all_on_focus = true
	name_input.custom_minimum_size = Vector2(300, 0)
	vbox.add_child(name_input)

	add_child(dialog)
	dialog.popup_centered()
	name_input.grab_focus()

	dialog.confirmed.connect(func():
		var new_name = name_input.text.strip_edges()
		if not new_name.is_empty():
			game_manager.rename_stage_design(stage_design_index, new_name)
		_pending_new = false
		_update_footer_button()
		_refresh_ui()
		dialog.queue_free()
	)
	dialog.canceled.connect(func():
		dialog.queue_free()
	)

func _on_create_variant():
	if not game_manager or stage_design_index < 0:
		return

	var dialog = ConfirmationDialog.new()
	dialog.title = "Create New Stage"

	var vbox = VBoxContainer.new()
	dialog.add_child(vbox)

	var label = Label.new()
	label.text = "Enter a name for the new stage:"
	vbox.add_child(label)

	var name_input = LineEdit.new()
	name_input.text = ""
	name_input.placeholder_text = "Stage name..."
	name_input.select_all_on_focus = true
	name_input.custom_minimum_size = Vector2(300, 0)
	vbox.add_child(name_input)

	add_child(dialog)
	dialog.popup_centered()
	name_input.grab_focus()

	dialog.confirmed.connect(func():
		var new_name = name_input.text.strip_edges()
		var new_idx = game_manager.duplicate_stage_design(stage_design_index)
		if new_idx >= 0:
			if not new_name.is_empty():
				game_manager.rename_stage_design(new_idx, new_name)
			stage_design_index = new_idx
			_pending_new = false
			_update_footer_button()
			_refresh_ui()
		dialog.queue_free()
	)
	dialog.canceled.connect(func():
		dialog.queue_free()
	)

# Helpers
func _format_money(value: float) -> String:
	if value >= 1_000_000_000:
		return "%.1fB" % (value / 1_000_000_000)
	elif value >= 1_000_000:
		return "%.0fM" % (value / 1_000_000)
	elif value >= 1_000:
		return "%.0fK" % (value / 1_000)
	else:
		return "%.0f" % value

func _format_mass(kg: float) -> String:
	if kg >= 1_000_000:
		return "%.1f kt" % (kg / 1_000_000)
	elif kg >= 1_000:
		return "%.1f t" % (kg / 1_000)
	else:
		return "%.0f kg" % kg
