extends Control

signal launch_requested
signal back_requested

# Designer node reference
@onready var designer: RocketDesigner = $RocketDesigner

# UI references - Header
@onready var target_dv_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderVBox/MissionInfo/TargetDV
@onready var payload_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderVBox/MissionInfo/Payload
@onready var budget_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderVBox/MissionInfo/Budget

# UI references - Main content
@onready var stages_container = $MarginContainer/VBox/ContentHBox/StagesPanel/StagesMargin/StagesVBox/StagesScroll/StagesList
@onready var engines_container = $MarginContainer/VBox/ContentHBox/EnginesPanel/EnginesMargin/EnginesVBox/EnginesList
@onready var stages_scroll = $MarginContainer/VBox/ContentHBox/StagesPanel/StagesMargin/StagesVBox/StagesScroll

# UI references - Footer
@onready var dv_progress = $MarginContainer/VBox/FooterPanel/FooterMargin/FooterVBox/DVProgress
@onready var dv_label = $MarginContainer/VBox/FooterPanel/FooterMargin/FooterVBox/DVInfo/DVLabel
@onready var dv_status = $MarginContainer/VBox/FooterPanel/FooterMargin/FooterVBox/DVInfo/DVStatus
@onready var success_label = $MarginContainer/VBox/FooterPanel/FooterMargin/FooterVBox/DVInfo/SuccessLabel
@onready var cost_label = $MarginContainer/VBox/FooterPanel/FooterMargin/FooterVBox/BudgetInfo/CostLabel
@onready var budget_status = $MarginContainer/VBox/FooterPanel/FooterMargin/FooterVBox/BudgetInfo/BudgetStatus
@onready var remaining_label = $MarginContainer/VBox/FooterPanel/FooterMargin/FooterVBox/BudgetInfo/RemainingLabel
@onready var launch_button = $MarginContainer/VBox/FooterPanel/FooterMargin/FooterVBox/ButtonsHBox/LaunchButton
@onready var back_button = $MarginContainer/VBox/FooterPanel/FooterMargin/FooterVBox/ButtonsHBox/BackButton

# Track if we're currently dragging a slider to prevent rebuild
var _slider_dragging: bool = false
var _slider_stage_index: int = -1

# Store stage card references for updates without rebuild
var _stage_cards: Array = []

func _ready():
	# Connect designer signal
	designer.design_changed.connect(_on_design_changed)

	# Set up engine cards
	_setup_engine_cards()

	# Load default design
	designer.load_default_design()

	# Update header
	_update_header()

	# Initial UI update
	_rebuild_stages_list()
	_update_dv_display()
	_update_budget_display()
	_update_launch_button()

func _setup_engine_cards():
	# Clear existing
	for child in engines_container.get_children():
		child.queue_free()

	# Create engine cards
	var engine_count = designer.get_engine_type_count()
	for i in range(engine_count):
		var card = _create_engine_card(i)
		engines_container.add_child(card)

func _create_engine_card(engine_type: int) -> Control:
	# Use a Control as container so we can implement _get_drag_data via script
	var container = Control.new()
	container.custom_minimum_size = Vector2(200, 160)  # Increased height for all content
	container.set_meta("engine_type", engine_type)

	var panel = PanelContainer.new()
	panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	panel.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	container.add_child(panel)

	var margin = MarginContainer.new()
	margin.mouse_filter = Control.MOUSE_FILTER_IGNORE
	margin.add_theme_constant_override("margin_left", 10)
	margin.add_theme_constant_override("margin_right", 10)
	margin.add_theme_constant_override("margin_top", 10)
	margin.add_theme_constant_override("margin_bottom", 10)
	panel.add_child(margin)

	var vbox = VBoxContainer.new()
	vbox.mouse_filter = Control.MOUSE_FILTER_IGNORE
	vbox.add_theme_constant_override("separation", 5)
	margin.add_child(vbox)

	# Engine name
	var name_label = Label.new()
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	name_label.text = designer.get_engine_name(engine_type)
	name_label.add_theme_font_size_override("font_size", 18)
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	vbox.add_child(name_label)

	# Stats
	var thrust = designer.get_engine_thrust(engine_type)
	var ve = designer.get_engine_exhaust_velocity(engine_type)
	var mass = designer.get_engine_mass(engine_type)
	var failure = designer.get_engine_failure_rate(engine_type) * 100
	var cost = designer.get_engine_cost(engine_type)

	var stats_label = Label.new()
	stats_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	stats_label.text = "Thrust: %.0f kN\nIsp: %.0f m/s\nMass: %.0f kg\nFailure: %.1f%%\nCost: $%sM" % [thrust, ve, mass, failure, _format_money(cost / 1000000)]
	stats_label.add_theme_font_size_override("font_size", 12)
	stats_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
	vbox.add_child(stats_label)

	# Add button instead of drag
	var add_btn = Button.new()
	add_btn.text = "ADD STAGE"
	add_btn.add_theme_font_size_override("font_size", 12)
	add_btn.pressed.connect(_on_add_engine_stage_pressed.bind(engine_type))
	vbox.add_child(add_btn)

	return container

func _on_add_engine_stage_pressed(engine_type: int):
	designer.add_stage(engine_type)


func _update_header():
	target_dv_label.text = "Target Δv: %.0f m/s" % designer.get_target_delta_v()
	payload_label.text = "Payload: %.0f kg" % designer.get_payload_mass()
	budget_label.text = "Budget: $%s" % _format_money(designer.get_starting_budget())

# Helper to format money values with commas
func _format_money(value: float) -> String:
	var int_value = int(value)
	var str_val = str(int_value)
	var result = ""
	var count = 0
	for i in range(str_val.length() - 1, -1, -1):
		if count > 0 and count % 3 == 0:
			result = "," + result
		result = str_val[i] + result
		count += 1
	return result

func _on_design_changed():
	# Don't rebuild if we're dragging a slider - just update values
	if _slider_dragging:
		_update_stage_values_only()
		_update_dv_display()
		_update_budget_display()
		_update_launch_button()
	else:
		_rebuild_stages_list()
		_update_dv_display()
		_update_budget_display()
		_update_launch_button()

func _update_stage_values_only():
	# Update values in existing stage cards without rebuilding
	var stage_count = designer.get_stage_count()
	for i in range(min(stage_count, _stage_cards.size())):
		var display_index = stage_count - 1 - i  # Reverse order
		var card_data = _stage_cards[i]
		if card_data.has("stage_index"):
			var stage_index = card_data["stage_index"]
			# Update TWR label
			if card_data.has("twr_label"):
				var twr = designer.get_stage_twr(stage_index)
				card_data["twr_label"].text = "TWR: %.2f" % twr
				# Color-code TWR
				if twr < 1.0:
					card_data["twr_label"].add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
				elif twr < 1.2:
					card_data["twr_label"].add_theme_color_override("font_color", Color(1.0, 1.0, 0.3))
				else:
					card_data["twr_label"].add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
			# Update delta-v label with effective delta-v and gravity loss
			if card_data.has("dv_label"):
				var effective_dv = designer.get_stage_effective_delta_v(stage_index)
				var gravity_loss = designer.get_stage_gravity_loss(stage_index)
				if gravity_loss > 0:
					card_data["dv_label"].text = "Δv: %.0f m/s (-%.0f)" % [effective_dv, gravity_loss]
				else:
					card_data["dv_label"].text = "Δv: %.0f m/s" % effective_dv
			# Update propellant mass label
			if card_data.has("prop_label"):
				var prop_mass = designer.get_stage_propellant_mass(stage_index)
				card_data["prop_label"].text = "Propellant: %.0f kg" % prop_mass
			# Update dry mass label
			if card_data.has("dry_label"):
				var dry_mass = designer.get_stage_dry_mass(stage_index)
				card_data["dry_label"].text = "Dry: %.0f kg" % dry_mass
			# Update cost label
			if card_data.has("cost_label"):
				var stage_cost = designer.get_stage_cost(stage_index)
				card_data["cost_label"].text = "Cost: $%s" % _format_money(stage_cost)
			# Update fraction label
			if card_data.has("frac_label"):
				var frac = designer.get_stage_mass_fraction(stage_index)
				card_data["frac_label"].text = "%.0f%%" % (frac * 100)

func _rebuild_stages_list():
	# Clear existing stage cards
	for child in stages_container.get_children():
		child.queue_free()
	_stage_cards.clear()

	var stage_count = designer.get_stage_count()

	if stage_count == 0:
		var drop_zone = _create_drop_zone_label()
		stages_container.add_child(drop_zone)
		return

	# Create stage cards in REVERSE order (last stage at top, first stage at bottom)
	# Stage 0 fires first, so it should be at the bottom
	for i in range(stage_count - 1, -1, -1):
		var card = _create_stage_card(i)
		stages_container.add_child(card)

func _create_drop_zone_label() -> Control:
	var container = Control.new()
	container.custom_minimum_size = Vector2(0, 200)
	container.size_flags_vertical = Control.SIZE_EXPAND_FILL

	var label = Label.new()
	label.text = "Drag engines here to create stages"
	label.add_theme_font_size_override("font_size", 16)
	label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.anchors_preset = Control.PRESET_FULL_RECT
	container.add_child(label)

	return container

func _create_stage_card(stage_index: int) -> PanelContainer:
	var panel = PanelContainer.new()
	panel.custom_minimum_size = Vector2(0, 140)
	panel.set_meta("stage_index", stage_index)
	panel.set_meta("is_stage_card", true)

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 15)
	margin.add_theme_constant_override("margin_right", 15)
	margin.add_theme_constant_override("margin_top", 10)
	margin.add_theme_constant_override("margin_bottom", 10)
	panel.add_child(margin)

	var vbox = VBoxContainer.new()
	vbox.add_theme_constant_override("separation", 8)
	margin.add_child(vbox)

	# Header row with move buttons, stage name, and remove button
	var header_hbox = HBoxContainer.new()
	vbox.add_child(header_hbox)

	# Move up button
	var move_up_btn = Button.new()
	move_up_btn.text = "▲"
	move_up_btn.custom_minimum_size = Vector2(30, 30)
	move_up_btn.tooltip_text = "Move stage up (fires later)"
	move_up_btn.pressed.connect(_on_move_stage_up_pressed.bind(stage_index))
	header_hbox.add_child(move_up_btn)

	# Move down button
	var move_down_btn = Button.new()
	move_down_btn.text = "▼"
	move_down_btn.custom_minimum_size = Vector2(30, 30)
	move_down_btn.tooltip_text = "Move stage down (fires earlier)"
	move_down_btn.pressed.connect(_on_move_stage_down_pressed.bind(stage_index))
	header_hbox.add_child(move_down_btn)

	var engine_type = designer.get_stage_engine_type(stage_index)
	var engine_name = designer.get_engine_name(engine_type)
	var stage_num = stage_index + 1

	var title_label = Label.new()
	title_label.text = " Stage %d: %s" % [stage_num, engine_name]
	title_label.add_theme_font_size_override("font_size", 16)
	title_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header_hbox.add_child(title_label)

	var remove_btn = Button.new()
	remove_btn.text = "X"
	remove_btn.custom_minimum_size = Vector2(30, 30)
	remove_btn.pressed.connect(_on_remove_stage_pressed.bind(stage_index))
	header_hbox.add_child(remove_btn)

	# Engine count row
	var engine_hbox = HBoxContainer.new()
	engine_hbox.add_theme_constant_override("separation", 10)
	vbox.add_child(engine_hbox)

	var engines_label = Label.new()
	engines_label.text = "Engines:"
	engines_label.add_theme_font_size_override("font_size", 14)
	engine_hbox.add_child(engines_label)

	var minus_btn = Button.new()
	minus_btn.text = "-"
	minus_btn.custom_minimum_size = Vector2(30, 30)
	minus_btn.pressed.connect(_on_engine_minus_pressed.bind(stage_index))
	engine_hbox.add_child(minus_btn)

	var count_label = Label.new()
	count_label.text = str(designer.get_stage_engine_count(stage_index))
	count_label.add_theme_font_size_override("font_size", 16)
	count_label.custom_minimum_size = Vector2(30, 0)
	count_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	engine_hbox.add_child(count_label)

	var plus_btn = Button.new()
	plus_btn.text = "+"
	plus_btn.custom_minimum_size = Vector2(30, 30)
	plus_btn.pressed.connect(_on_engine_plus_pressed.bind(stage_index))
	engine_hbox.add_child(plus_btn)

	# Spacer
	var spacer = Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	engine_hbox.add_child(spacer)

	# TWR for this stage
	var twr = designer.get_stage_twr(stage_index)
	var twr_label = Label.new()
	twr_label.text = "TWR: %.2f" % twr
	twr_label.add_theme_font_size_override("font_size", 14)
	# Color-code TWR: red if can't lift off, yellow if marginal, green if good
	if twr < 1.0:
		twr_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
	elif twr < 1.2:
		twr_label.add_theme_color_override("font_color", Color(1.0, 1.0, 0.3))
	else:
		twr_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
	engine_hbox.add_child(twr_label)

	# Small spacer
	var spacer2 = Control.new()
	spacer2.custom_minimum_size = Vector2(15, 0)
	engine_hbox.add_child(spacer2)

	# Effective delta-v for this stage (after gravity losses)
	var effective_dv = designer.get_stage_effective_delta_v(stage_index)
	var ideal_dv = designer.get_stage_delta_v(stage_index)
	var gravity_loss = designer.get_stage_gravity_loss(stage_index)
	var dv_label_stage = Label.new()
	if gravity_loss > 0:
		dv_label_stage.text = "Δv: %.0f m/s (-%0.f)" % [effective_dv, gravity_loss]
	else:
		dv_label_stage.text = "Δv: %.0f m/s" % effective_dv
	dv_label_stage.add_theme_font_size_override("font_size", 14)
	dv_label_stage.add_theme_color_override("font_color", Color(0.3, 0.8, 1.0))
	engine_hbox.add_child(dv_label_stage)

	# Mass fraction slider row
	var slider_hbox = HBoxContainer.new()
	slider_hbox.add_theme_constant_override("separation", 10)
	vbox.add_child(slider_hbox)

	var frac_label = Label.new()
	frac_label.text = "Propellant:"
	frac_label.add_theme_font_size_override("font_size", 14)
	slider_hbox.add_child(frac_label)

	var slider = HSlider.new()
	slider.min_value = 0.5
	slider.max_value = 0.95
	slider.step = 0.01
	slider.value = designer.get_stage_mass_fraction(stage_index)
	slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	slider.custom_minimum_size = Vector2(150, 0)
	# Connect to drag_started and drag_ended to track slider state
	slider.drag_started.connect(_on_slider_drag_started.bind(stage_index))
	slider.drag_ended.connect(_on_slider_drag_ended.bind(stage_index))
	slider.value_changed.connect(_on_mass_fraction_changed.bind(stage_index))
	slider_hbox.add_child(slider)

	var frac_value_label = Label.new()
	frac_value_label.text = "%.0f%%" % (slider.value * 100)
	frac_value_label.add_theme_font_size_override("font_size", 14)
	frac_value_label.custom_minimum_size = Vector2(50, 0)
	slider_hbox.add_child(frac_value_label)

	# Stage info row (propellant, dry mass, and cost)
	var info_hbox = HBoxContainer.new()
	info_hbox.add_theme_constant_override("separation", 20)
	vbox.add_child(info_hbox)

	var prop_mass = designer.get_stage_propellant_mass(stage_index)
	var prop_label = Label.new()
	prop_label.text = "Propellant: %.0f kg" % prop_mass
	prop_label.add_theme_font_size_override("font_size", 12)
	prop_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	info_hbox.add_child(prop_label)

	# Dry mass (engines + tank structure)
	var dry_mass = designer.get_stage_dry_mass(stage_index)
	var dry_label = Label.new()
	dry_label.text = "Dry: %.0f kg" % dry_mass
	dry_label.add_theme_font_size_override("font_size", 12)
	dry_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	info_hbox.add_child(dry_label)

	# Stage cost
	var stage_cost = designer.get_stage_cost(stage_index)
	var cost_label_stage = Label.new()
	cost_label_stage.text = "Cost: $%s" % _format_money(stage_cost)
	cost_label_stage.add_theme_font_size_override("font_size", 12)
	cost_label_stage.add_theme_color_override("font_color", Color(1.0, 0.85, 0.3))
	info_hbox.add_child(cost_label_stage)

	# Store references for updating without rebuild
	var card_data = {
		"stage_index": stage_index,
		"dv_label": dv_label_stage,
		"twr_label": twr_label,
		"prop_label": prop_label,
		"dry_label": dry_label,
		"cost_label": cost_label_stage,
		"frac_label": frac_value_label,
		"slider": slider
	}
	_stage_cards.append(card_data)

	return panel

func _on_slider_drag_started(stage_index: int):
	_slider_dragging = true
	_slider_stage_index = stage_index

func _on_slider_drag_ended(value_changed: bool, stage_index: int):
	_slider_dragging = false
	_slider_stage_index = -1
	# Do a full rebuild now that dragging is done
	if value_changed:
		_rebuild_stages_list()
		_update_dv_display()

func _on_move_stage_up_pressed(stage_index: int):
	# Move stage up means it fires later (higher index)
	var stage_count = designer.get_stage_count()
	if stage_index < stage_count - 1:
		designer.move_stage(stage_index, stage_index + 1)

func _on_move_stage_down_pressed(stage_index: int):
	# Move stage down means it fires earlier (lower index)
	if stage_index > 0:
		designer.move_stage(stage_index, stage_index - 1)

func _on_remove_stage_pressed(stage_index: int):
	designer.remove_stage(stage_index)

func _on_engine_minus_pressed(stage_index: int):
	var current = designer.get_stage_engine_count(stage_index)
	if current > 1:
		designer.set_stage_engine_count(stage_index, current - 1)

func _on_engine_plus_pressed(stage_index: int):
	var current = designer.get_stage_engine_count(stage_index)
	if current < 9:
		designer.set_stage_engine_count(stage_index, current + 1)

func _on_mass_fraction_changed(value: float, stage_index: int):
	designer.set_stage_mass_fraction(stage_index, value)
	# Update the fraction label immediately
	for card_data in _stage_cards:
		if card_data.get("stage_index") == stage_index and card_data.has("frac_label"):
			card_data["frac_label"].text = "%.0f%%" % (value * 100)
			break

func _update_dv_display():
	var effective_dv = designer.get_total_effective_delta_v()
	var ideal_dv = designer.get_total_delta_v()
	var gravity_loss = designer.get_total_gravity_loss()
	var target_dv = designer.get_target_delta_v()
	var percentage = designer.get_delta_v_percentage()
	var success_prob = designer.get_mission_success_probability() * 100

	# Show effective delta-v with gravity loss in parentheses
	if gravity_loss > 0:
		dv_label.text = "Δv: %.0f / %.0f m/s (gravity loss: -%.0f)" % [effective_dv, target_dv, gravity_loss]
	else:
		dv_label.text = "Δv: %.0f / %.0f m/s" % [effective_dv, target_dv]
	dv_progress.value = min(percentage, 100)

	if designer.is_design_sufficient():
		dv_status.text = "SUFFICIENT"
		dv_status.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
		dv_progress.modulate = Color(0.3, 1.0, 0.3)
	else:
		dv_status.text = "INSUFFICIENT"
		dv_status.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
		dv_progress.modulate = Color(1.0, 0.3, 0.3)

	if designer.is_design_valid():
		success_label.text = "Success Rate: %.1f%%" % success_prob
	else:
		success_label.text = ""

func _update_budget_display():
	var total_cost = designer.get_total_cost()
	var remaining = designer.get_remaining_budget()

	cost_label.text = "Cost: $%s" % _format_money(total_cost)
	remaining_label.text = "Remaining: $%s" % _format_money(remaining)

	if designer.is_within_budget():
		budget_status.text = "WITHIN BUDGET"
		budget_status.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
		remaining_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
	else:
		budget_status.text = "OVER BUDGET"
		budget_status.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
		remaining_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))

func _update_launch_button():
	# Rocket must have sufficient delta-v AND be within budget to launch
	launch_button.disabled = not designer.is_launchable()

func _on_launch_button_pressed():
	launch_requested.emit()

func _on_back_button_pressed():
	back_requested.emit()

# Called by main scene to get the designer node
func get_designer() -> RocketDesigner:
	return designer
