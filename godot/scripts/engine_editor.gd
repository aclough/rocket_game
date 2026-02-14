extends Control

signal back_requested

var game_manager: GameManager = null
var engine_index: int = -1

# When true, the engine was created as a temporary placeholder for preview.
# BACK deletes it; SAVE keeps it and prompts for a name.
var _pending_new: bool = false

# UI references (built dynamically)
var _name_label: Label
var _rename_btn: Button
var _status_label: Label
var _fuel_buttons: Array[Button] = []
var _scale_slider: HSlider
var _scale_label: Label
var _cycle_option: OptionButton
var _stats_labels: Dictionary = {}
var _config_container: VBoxContainer
var _footer_btn: Button

# Fuel type names matching FuelType enum order
const FUEL_TYPES = ["Kerolox", "Hydrolox", "Solid", "Methalox", "Hypergolic"]

func _ready():
	_build_ui()

func set_game_manager(gm: GameManager):
	game_manager = gm

func load_engine(index: int):
	if index < 0:
		# Create mode: make a temporary engine for live preview
		if game_manager:
			engine_index = game_manager.create_engine_design(0, 1.0)
		_pending_new = true
	else:
		engine_index = index
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
	title.text = "ENGINE DESIGNER"
	title.add_theme_font_size_override("font_size", 22)
	title_hbox.add_child(title)

	_name_label = Label.new()
	_name_label.text = "- Engine Name"
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

	# === Category Panel (Chemical) ===
	var category_panel = PanelContainer.new()
	var cat_style = StyleBoxFlat.new()
	cat_style.set_bg_color(Color(0.1, 0.1, 0.08))
	cat_style.set_border_width_all(1)
	cat_style.set_border_color(Color(0.4, 0.35, 0.2, 0.5))
	category_panel.add_theme_stylebox_override("panel", cat_style)
	root_vbox.add_child(category_panel)

	var cat_margin = MarginContainer.new()
	cat_margin.add_theme_constant_override("margin_left", 15)
	cat_margin.add_theme_constant_override("margin_right", 15)
	cat_margin.add_theme_constant_override("margin_top", 10)
	cat_margin.add_theme_constant_override("margin_bottom", 10)
	category_panel.add_child(cat_margin)

	var cat_vbox = VBoxContainer.new()
	cat_vbox.add_theme_constant_override("separation", 10)
	cat_margin.add_child(cat_vbox)

	var cat_label = Label.new()
	cat_label.text = "Category: Chemical"
	cat_label.add_theme_font_size_override("font_size", 16)
	cat_label.add_theme_color_override("font_color", Color(1.0, 0.9, 0.7))
	cat_vbox.add_child(cat_label)

	# Config container - swapped per category (for now just Chemical)
	_config_container = VBoxContainer.new()
	_config_container.add_theme_constant_override("separation", 12)
	cat_vbox.add_child(_config_container)

	_build_chemical_config()

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

	var stat_names = ["Thrust", "Exhaust Velocity", "Mass", "Cost", "Complexity", "Propellant", "Tank Ratio"]
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
	# Hide RENAME button for pending new engines (no name yet)
	if _rename_btn:
		_rename_btn.visible = not _pending_new

func _build_chemical_config():
	# Clear existing config
	for child in _config_container.get_children():
		child.queue_free()
	_fuel_buttons.clear()

	# Fuel type selector
	var fuel_hbox = HBoxContainer.new()
	fuel_hbox.add_theme_constant_override("separation", 10)
	_config_container.add_child(fuel_hbox)

	var fuel_label = Label.new()
	fuel_label.text = "Fuel:"
	fuel_label.add_theme_font_size_override("font_size", 14)
	fuel_hbox.add_child(fuel_label)

	for i in range(FUEL_TYPES.size()):
		var btn = Button.new()
		btn.text = FUEL_TYPES[i]
		btn.custom_minimum_size = Vector2(100, 35)
		btn.add_theme_font_size_override("font_size", 14)
		btn.toggle_mode = true
		btn.pressed.connect(_on_fuel_type_selected.bind(i))
		fuel_hbox.add_child(btn)
		_fuel_buttons.append(btn)

	# Scale slider
	var scale_hbox = HBoxContainer.new()
	scale_hbox.add_theme_constant_override("separation", 10)
	_config_container.add_child(scale_hbox)

	var scale_title = Label.new()
	scale_title.text = "Scale:"
	scale_title.add_theme_font_size_override("font_size", 14)
	scale_hbox.add_child(scale_title)

	_scale_slider = HSlider.new()
	_scale_slider.min_value = 0.25
	_scale_slider.max_value = 4.0
	_scale_slider.step = 0.25
	_scale_slider.value = 1.0
	_scale_slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scale_slider.custom_minimum_size = Vector2(200, 0)
	_scale_slider.value_changed.connect(_on_scale_changed)
	scale_hbox.add_child(_scale_slider)

	_scale_label = Label.new()
	_scale_label.text = "1.00x"
	_scale_label.add_theme_font_size_override("font_size", 14)
	_scale_label.custom_minimum_size = Vector2(60, 0)
	scale_hbox.add_child(_scale_label)

	var range_label = Label.new()
	range_label.text = "Range: 0.25x - 4.00x"
	range_label.add_theme_font_size_override("font_size", 11)
	range_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	_config_container.add_child(range_label)

	# Engine cycle selector
	var cycle_hbox = HBoxContainer.new()
	cycle_hbox.add_theme_constant_override("separation", 10)
	_config_container.add_child(cycle_hbox)

	var cycle_title = Label.new()
	cycle_title.text = "Cycle:"
	cycle_title.add_theme_font_size_override("font_size", 14)
	cycle_hbox.add_child(cycle_title)

	_cycle_option = OptionButton.new()
	_cycle_option.custom_minimum_size = Vector2(220, 0)
	_cycle_option.add_theme_font_size_override("font_size", 14)
	_cycle_option.item_selected.connect(_on_cycle_selected)
	cycle_hbox.add_child(_cycle_option)

	var cycle_hint = Label.new()
	cycle_hint.text = "Higher-performance cycles increase cost, build time, and flaws"
	cycle_hint.add_theme_font_size_override("font_size", 11)
	cycle_hint.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	_config_container.add_child(cycle_hint)

func _refresh_ui():
	if not game_manager or engine_index < 0:
		return

	var engine_name = game_manager.get_engine_type_name(engine_index)
	var status = game_manager.get_engine_status(engine_index)
	var base_status = game_manager.get_engine_status_base(engine_index)
	var fuel_type = game_manager.get_engine_design_fuel_type(engine_index)
	var engine_scale = game_manager.get_engine_design_scale(engine_index)
	var can_modify = game_manager.can_modify_engine_design(engine_index)

	# Header
	if _pending_new:
		_name_label.text = "- New Engine (unsaved)"
		_status_label.text = "Configure and click SAVE"
		_status_label.add_theme_color_override("font_color", Color(0.8, 0.8, 0.4))
	else:
		_name_label.text = "- " + engine_name
		_status_label.text = "Status: " + status
		if base_status == "Specification":
			_status_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		elif base_status == "Engineering":
			_status_label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
		elif base_status == "Testing":
			_status_label.add_theme_color_override("font_color", Color(0.4, 0.6, 1.0))
		elif base_status == "Fixing":
			_status_label.add_theme_color_override("font_color", Color(1.0, 0.7, 0.3))

	# Fuel buttons
	for i in range(_fuel_buttons.size()):
		_fuel_buttons[i].button_pressed = (i == fuel_type)
		_fuel_buttons[i].disabled = not can_modify

	# Scale slider
	_scale_slider.set_value_no_signal(engine_scale)
	_scale_label.text = "%.2fx" % engine_scale
	_scale_slider.editable = can_modify

	# Cycle selector
	var current_cycle = game_manager.get_engine_design_cycle(engine_index)
	_cycle_option.clear()
	var valid_count = game_manager.get_valid_cycle_count(engine_index)
	var selected_option = 0
	for i in range(valid_count):
		var cycle_index = game_manager.get_valid_cycle_index(engine_index, i)
		var cycle_name = game_manager.get_valid_cycle_name(engine_index, i)
		_cycle_option.add_item(cycle_name, cycle_index)
		if cycle_index == current_cycle:
			selected_option = i
	_cycle_option.selected = selected_option
	_cycle_option.disabled = not can_modify

	# Stats
	_update_stats()

func _update_stats():
	if not game_manager or engine_index < 0:
		return

	var thrust = game_manager.get_engine_design_thrust(engine_index)
	var ve = game_manager.get_engine_design_exhaust_velocity(engine_index)
	var mass = game_manager.get_engine_design_mass(engine_index)
	var cost = game_manager.get_engine_design_cost(engine_index)
	var fuel_name = game_manager.get_engine_design_fuel_type_name(engine_index)

	_stats_labels["Thrust"].text = "Thrust: %.0f kN" % thrust
	_stats_labels["Exhaust Velocity"].text = "Exhaust Velocity: %.0f m/s" % ve
	if mass >= 1000:
		_stats_labels["Mass"].text = "Mass: %.1f t" % (mass / 1000.0)
	else:
		_stats_labels["Mass"].text = "Mass: %.0f kg" % mass
	_stats_labels["Cost"].text = "Cost: $%.1fM" % (cost / 1_000_000.0)
	var complexity = game_manager.get_engine_design_complexity(engine_index)
	_stats_labels["Complexity"].text = "Complexity: %d" % complexity
	_stats_labels["Propellant"].text = "Propellant: %s" % fuel_name
	var fuel_type = game_manager.get_engine_design_fuel_type(engine_index)
	match fuel_type:
		0:  # Kerolox
			_stats_labels["Tank Ratio"].text = "Tank Ratio: 6%"
		1:  # Hydrolox
			_stats_labels["Tank Ratio"].text = "Tank Ratio: 10%"
		2:  # Solid
			_stats_labels["Tank Ratio"].text = "Tank Ratio: 13.6% (fixed mass ratio)"
		3:  # Methalox
			_stats_labels["Tank Ratio"].text = "Tank Ratio: 7%"
		4:  # Hypergolic
			_stats_labels["Tank Ratio"].text = "Tank Ratio: 5%"

# === Signal handlers ===

func _on_fuel_type_selected(fuel_index: int):
	if game_manager and engine_index >= 0:
		game_manager.set_engine_design_fuel_type(engine_index, fuel_index)
		# Fuel type change may switch cycle, so do a full refresh
		_refresh_ui()

func _on_scale_changed(value: float):
	if game_manager and engine_index >= 0:
		game_manager.set_engine_design_scale(engine_index, value)
		_scale_label.text = "%.2fx" % value
		_update_stats()

func _on_cycle_selected(option_index: int):
	if game_manager and engine_index >= 0:
		var cycle_index = game_manager.get_valid_cycle_index(engine_index, option_index)
		game_manager.set_engine_design_cycle(engine_index, cycle_index)
		_refresh_ui()

func _on_rename_pressed():
	if not game_manager or engine_index < 0:
		return

	var dialog = ConfirmationDialog.new()
	dialog.title = "Rename Engine"

	var vbox = VBoxContainer.new()
	dialog.add_child(vbox)

	var label = Label.new()
	label.text = "Enter a new name:"
	vbox.add_child(label)

	var name_input = LineEdit.new()
	name_input.text = game_manager.get_engine_type_name(engine_index)
	name_input.select_all_on_focus = true
	name_input.custom_minimum_size = Vector2(300, 0)
	vbox.add_child(name_input)

	add_child(dialog)
	dialog.popup_centered()
	name_input.grab_focus()

	dialog.confirmed.connect(func():
		var new_name = name_input.text.strip_edges()
		if not new_name.is_empty():
			game_manager.rename_engine_design(engine_index, new_name)
			_refresh_ui()
		dialog.queue_free()
	)
	dialog.canceled.connect(func():
		dialog.queue_free()
	)

func _on_back_pressed():
	# If we were creating a new engine and the user backs out, discard it
	if _pending_new and game_manager and engine_index >= 0:
		game_manager.delete_engine_design(engine_index)
	_pending_new = false
	back_requested.emit()

func _on_footer_btn_pressed():
	if _pending_new:
		_on_save_new_engine()
	else:
		_on_create_variant()

func _on_save_new_engine():
	# Prompt for a name, then keep the pending engine
	if not game_manager or engine_index < 0:
		return

	var dialog = ConfirmationDialog.new()
	dialog.title = "Save Engine"

	var vbox = VBoxContainer.new()
	dialog.add_child(vbox)

	var label = Label.new()
	label.text = "Enter a name for this engine:"
	vbox.add_child(label)

	var name_input = LineEdit.new()
	name_input.text = ""
	name_input.placeholder_text = "Engine name..."
	name_input.select_all_on_focus = true
	name_input.custom_minimum_size = Vector2(300, 0)
	vbox.add_child(name_input)

	add_child(dialog)
	dialog.popup_centered()
	name_input.grab_focus()

	dialog.confirmed.connect(func():
		var new_name = name_input.text.strip_edges()
		if not new_name.is_empty():
			game_manager.rename_engine_design(engine_index, new_name)
		_pending_new = false
		_update_footer_button()
		_refresh_ui()
		dialog.queue_free()
	)
	dialog.canceled.connect(func():
		dialog.queue_free()
	)

func _on_create_variant():
	# Create a new engine based on the current one's settings
	if not game_manager or engine_index < 0:
		return

	var current_fuel = game_manager.get_engine_design_fuel_type(engine_index)
	var current_scale = game_manager.get_engine_design_scale(engine_index)

	var dialog = ConfirmationDialog.new()
	dialog.title = "Create New Engine"

	var vbox = VBoxContainer.new()
	dialog.add_child(vbox)

	var label = Label.new()
	label.text = "Enter a name for the new engine:"
	vbox.add_child(label)

	var name_input = LineEdit.new()
	name_input.text = ""
	name_input.placeholder_text = "Engine name..."
	name_input.select_all_on_focus = true
	name_input.custom_minimum_size = Vector2(300, 0)
	vbox.add_child(name_input)

	add_child(dialog)
	dialog.popup_centered()
	name_input.grab_focus()

	dialog.confirmed.connect(func():
		var new_name = name_input.text.strip_edges()
		var new_idx = game_manager.create_engine_design(current_fuel, current_scale)
		if new_idx >= 0:
			if not new_name.is_empty():
				game_manager.rename_engine_design(new_idx, new_name)
			engine_index = new_idx
			_refresh_ui()
		dialog.queue_free()
	)
	dialog.canceled.connect(func():
		dialog.queue_free()
	)
