extends Control

signal launch_requested
signal back_requested
signal testing_requested
signal submit_to_engineering_requested
signal design_saved

# Game manager reference (set by parent)
var game_manager: GameManager = null

# Designer node reference
@onready var designer: RocketDesigner = $RocketDesigner
@onready var save_button = $MarginContainer/VBox/FooterPanel/FooterMargin/FooterVBox/ButtonsHBox/SaveButton

# UI references - Header
@onready var design_name_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderVBox/TitleHBox/DesignNameLabel
@onready var target_dv_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderVBox/MissionInfo/TargetDV
@onready var payload_value_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderVBox/MissionInfo/PayloadContainer/PayloadValue
@onready var payload_decrease_btn = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderVBox/MissionInfo/PayloadContainer/PayloadDecreaseBtn
@onready var payload_increase_btn = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderVBox/MissionInfo/PayloadContainer/PayloadIncreaseBtn

# Payload adjustment increment in kg
const PAYLOAD_INCREMENT: float = 1000.0
const PAYLOAD_MIN: float = 100.0
const PAYLOAD_MAX: float = 50000.0

# UI references - Main content
@onready var stages_container = $MarginContainer/VBox/ContentHBox/StagesPanel/StagesMargin/StagesVBox/StagesScroll/StagesList
@onready var engines_container = $MarginContainer/VBox/ContentHBox/EnginesPanel/EnginesMargin/EnginesVBox/EnginesScroll/EnginesList
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

	# Connect visibility changed to update header when screen shown
	visibility_changed.connect(_on_visibility_changed)

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

func _on_visibility_changed():
	if visible:
		# Update header when screen becomes visible (payload/target may have changed)
		_update_header()
		_update_dv_display()
		# Refresh engine cards in case engines were added/modified
		_setup_engine_cards()

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
	var cost = designer.get_engine_cost(engine_type)
	var is_solid = designer.is_engine_type_solid(engine_type)

	# Get manufacturing info if available
	var material_cost_str = "$%sM" % _format_money_short(cost * 0.4 / 1_000_000.0)
	var build_days_str = ""
	if game_manager:
		var mat_cost = game_manager.get_engine_material_cost(engine_type)
		var build_days = game_manager.get_engine_build_days(engine_type)
		material_cost_str = "$%sM" % _format_money_short(mat_cost / 1_000_000.0)
		build_days_str = " | Build: ~%.0f days" % build_days

	var stats_label = Label.new()
	stats_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	if is_solid:
		# Solid motors show different info
		stats_label.text = "Thrust: %.0f kN\nIsp: %.0f m/s\nMotor: %.0f kg\nFixed ratio: 88%%\nMaterial: %s%s" % [thrust, ve, mass, material_cost_str, build_days_str]
		stats_label.add_theme_color_override("font_color", Color(1.0, 0.7, 0.4))
	else:
		stats_label.text = "Thrust: %.0f kN\nIsp: %.0f m/s\nMass: %.0f kg\nMaterial: %s%s" % [thrust, ve, mass, material_cost_str, build_days_str]
		stats_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
	stats_label.add_theme_font_size_override("font_size", 12)
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
	design_name_label.text = "- " + designer.get_design_name()
	target_dv_label.text = "Target Δv: %.0f m/s" % designer.get_target_delta_v()

	var payload = designer.get_payload_mass()
	payload_value_label.text = _format_number(payload) + " kg"

	# Enable/disable payload buttons based on whether there's an active contract
	var has_contract = game_manager and game_manager.has_active_contract()
	payload_decrease_btn.visible = not has_contract
	payload_increase_btn.visible = not has_contract

	# Also disable at min/max limits
	if not has_contract:
		payload_decrease_btn.disabled = payload <= PAYLOAD_MIN
		payload_increase_btn.disabled = payload >= PAYLOAD_MAX

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

# Helper to format numbers with commas (alias for non-money values)
func _format_number(value: float) -> String:
	return _format_money(value)

# Helper to format money in compact form (e.g., 41.1)
func _format_money_short(value: float) -> String:
	if value >= 100:
		return "%.0f" % value
	elif value >= 10:
		return "%.1f" % value
	else:
		return "%.2f" % value

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
		var _display_index = stage_count - 1 - i  # Reverse order
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
				if card_data.get("is_solid", false):
					card_data["dry_label"].text = "Motor Mass: %.0f kg" % dry_mass
				else:
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
	var is_booster = designer.is_stage_booster(stage_index)

	var panel = PanelContainer.new()
	panel.custom_minimum_size = Vector2(0, 140)
	panel.set_meta("stage_index", stage_index)
	panel.set_meta("is_stage_card", true)

	# Visual styling for boosters - indent and add a tint
	if is_booster:
		var style = StyleBoxFlat.new()
		style.set_bg_color(Color(0.15, 0.12, 0.08))  # Slight orange/brown tint
		style.set_border_width_all(1)
		style.set_border_color(Color(1.0, 0.8, 0.2, 0.5))  # Orange border
		style.content_margin_left = 25  # Indent booster cards
		panel.add_theme_stylebox_override("panel", style)

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 15 if not is_booster else 25)
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

	# OUT button (make parallel/booster)
	var out_btn = Button.new()
	out_btn.text = "⇥" if not is_booster else "⇤"
	out_btn.custom_minimum_size = Vector2(30, 30)
	out_btn.tooltip_text = "Remove from parallel" if is_booster else "Make parallel with stage below (booster)"
	out_btn.pressed.connect(_on_toggle_booster_pressed.bind(stage_index))
	# Can only make a booster if not the first stage
	out_btn.visible = stage_index > 0
	# Highlight button if this is a booster
	if is_booster:
		out_btn.add_theme_color_override("font_color", Color(1.0, 0.8, 0.2))
	header_hbox.add_child(out_btn)

	var engine_type = designer.get_stage_engine_type(stage_index)
	var engine_name = designer.get_engine_name(engine_type)

	# Calculate actual stage number (only counting non-booster stages)
	var stage_num = 0
	for i in range(stage_index + 1):
		if not designer.is_stage_booster(i):
			stage_num += 1

	var title_label = Label.new()
	if is_booster:
		# Find the core stage this booster is attached to
		var core_index = stage_index - 1
		while core_index > 0 and designer.is_stage_booster(core_index):
			core_index -= 1
		# Calculate the core's stage number
		var core_stage_num = 0
		for i in range(core_index + 1):
			if not designer.is_stage_booster(i):
				core_stage_num += 1
		title_label.text = " Stage %d Booster: %s" % [core_stage_num, engine_name]
		title_label.add_theme_color_override("font_color", Color(1.0, 0.8, 0.2))
	else:
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
	twr_label.add_theme_font_size_override("font_size", 14)

	if is_booster:
		# Booster TWR is combined with core
		twr_label.text = "(parallel)"
		twr_label.add_theme_color_override("font_color", Color(0.7, 0.6, 0.4))
	else:
		twr_label.text = "TWR: %.2f" % twr
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
	dv_label_stage.add_theme_font_size_override("font_size", 14)

	if is_booster:
		# Booster delta-v is combined with core
		dv_label_stage.text = "(with core)"
		dv_label_stage.add_theme_color_override("font_color", Color(0.7, 0.6, 0.4))
	elif gravity_loss > 0:
		dv_label_stage.text = "Δv: %.0f m/s (-%0.f)" % [effective_dv, gravity_loss]
		dv_label_stage.add_theme_color_override("font_color", Color(0.3, 0.8, 1.0))
	else:
		dv_label_stage.text = "Δv: %.0f m/s" % effective_dv
		dv_label_stage.add_theme_color_override("font_color", Color(0.3, 0.8, 1.0))
	engine_hbox.add_child(dv_label_stage)

	# Mass fraction slider row (or fixed info for solids)
	var slider_hbox = HBoxContainer.new()
	slider_hbox.add_theme_constant_override("separation", 10)
	vbox.add_child(slider_hbox)

	var is_solid = designer.is_stage_solid(stage_index)
	var slider: HSlider = null
	var frac_value_label: Label = null

	if is_solid:
		# Solid motors have fixed mass ratio - show info instead of slider
		var solid_label = Label.new()
		solid_label.text = "Solid Motor (fixed mass ratio 88%)"
		solid_label.add_theme_font_size_override("font_size", 14)
		solid_label.add_theme_color_override("font_color", Color(1.0, 0.6, 0.2))
		slider_hbox.add_child(solid_label)
	else:
		# Liquid engines - show adjustable propellant slider
		var frac_label = Label.new()
		frac_label.text = "Propellant:"
		frac_label.add_theme_font_size_override("font_size", 14)
		slider_hbox.add_child(frac_label)

		slider = HSlider.new()
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

		frac_value_label = Label.new()
		frac_value_label.text = "%.0f%%" % (slider.value * 100)
		frac_value_label.add_theme_font_size_override("font_size", 14)
		frac_value_label.custom_minimum_size = Vector2(50, 0)
		slider_hbox.add_child(frac_value_label)

	# Stage info row (propellant, dry mass, and cost)
	var info_hbox = HBoxContainer.new()
	info_hbox.add_theme_constant_override("separation", 20)
	vbox.add_child(info_hbox)

	var prop_mass = designer.get_stage_propellant_mass(stage_index)
	var dry_mass = designer.get_stage_dry_mass(stage_index)
	var prop_label = Label.new()
	var dry_label = Label.new()

	if is_solid:
		# For solid motors, show motor mass (dry) and propellant (auto-calculated)
		dry_label.text = "Motor Mass: %.0f kg" % dry_mass
		dry_label.add_theme_font_size_override("font_size", 12)
		dry_label.add_theme_color_override("font_color", Color(1.0, 0.6, 0.2))
		info_hbox.add_child(dry_label)

		prop_label.text = "Propellant: %.0f kg" % prop_mass
		prop_label.add_theme_font_size_override("font_size", 12)
		prop_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
		info_hbox.add_child(prop_label)
	else:
		# For liquid stages, show propellant first then dry mass
		prop_label.text = "Propellant: %.0f kg" % prop_mass
		prop_label.add_theme_font_size_override("font_size", 12)
		prop_label.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
		info_hbox.add_child(prop_label)

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
		"is_solid": is_solid,
	}
	# Only store slider references for non-solid stages
	if not is_solid:
		card_data["frac_label"] = frac_value_label
		card_data["slider"] = slider
	_stage_cards.append(card_data)

	return panel

func _on_slider_drag_started(stage_index: int):
	_slider_dragging = true
	_slider_stage_index = stage_index

func _on_slider_drag_ended(value_changed: bool, _stage_index: int):
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

func _on_toggle_booster_pressed(stage_index: int):
	var is_booster = designer.is_stage_booster(stage_index)
	if is_booster:
		# Remove booster status
		designer.set_stage_booster(stage_index, false)
	else:
		# Check if can be booster
		if designer.can_be_booster(stage_index):
			designer.set_stage_booster(stage_index, true)
		else:
			# Show error message
			var error = designer.get_booster_validation_error(stage_index)
			_show_booster_error(error)

func _show_booster_error(error: String):
	# Create a simple popup to show the error
	var popup = AcceptDialog.new()
	popup.title = "Cannot Set Booster"
	popup.dialog_text = error
	popup.dialog_hide_on_ok = true
	add_child(popup)
	popup.popup_centered()
	# Clean up popup when closed
	popup.confirmed.connect(func(): popup.queue_free())
	popup.canceled.connect(func(): popup.queue_free())

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

	# Show both the launch cost and manufacturing info
	if game_manager:
		var current_index = game_manager.get_current_design_index()
		if current_index >= 0:
			var material_cost = game_manager.get_rocket_material_cost(current_index)
			var assembly_days = game_manager.get_rocket_assembly_days(current_index)
			cost_label.text = "Materials: $%sM | Build: ~%.0f team-days" % [_format_money_short(material_cost / 1_000_000.0), assembly_days]

			# Show engines required
			var engines_req = game_manager.get_engines_required_for_rocket(current_index)
			if engines_req.size() > 0:
				var parts = []
				for req in engines_req:
					var eng_name = req.get("name", "?")
					var eng_count = req.get("count", 0)
					parts.append("%dx %s" % [eng_count, eng_name])
				remaining_label.text = "Engines: %s" % ", ".join(parts)
				remaining_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
			else:
				remaining_label.text = ""
		else:
			cost_label.text = "Cost: $%s" % _format_money(total_cost)
			remaining_label.text = "Remaining: $%s" % _format_money(remaining)
	else:
		cost_label.text = "Cost: $%s" % _format_money(total_cost)
		remaining_label.text = "Remaining: $%s" % _format_money(remaining)

	if designer.is_within_budget():
		budget_status.text = "WITHIN BUDGET"
		budget_status.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
	else:
		budget_status.text = "OVER BUDGET"
		budget_status.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))

func _update_launch_button():
	# Check design status to determine button behavior
	var status = ""
	if game_manager:
		status = game_manager.get_current_design_status()

	# Default: not launchable
	launch_button.disabled = true

	if status == "Specification" or status == "":
		# Design needs to be submitted to engineering
		launch_button.text = "SUBMIT TO ENGINEERING"
		launch_button.disabled = not designer.is_launchable()
	elif status == "Complete":
		# Design is complete, can proceed to testing
		launch_button.text = "CONTINUE TO TESTING"
		launch_button.disabled = not designer.is_launchable()
	else:
		# Design is in Engineering or Testing - can't proceed yet
		launch_button.text = status.to_upper() + " IN PROGRESS"
		launch_button.disabled = true

func _on_launch_button_pressed():
	var status = ""
	if game_manager:
		status = game_manager.get_current_design_status()

	if status == "Specification" or status == "":
		# Submit to engineering
		submit_to_engineering_requested.emit()
	elif status == "Complete":
		# Go to testing screen
		testing_requested.emit()

func _on_payload_decrease_pressed():
	var current = designer.get_payload_mass()
	var new_payload = max(PAYLOAD_MIN, current - PAYLOAD_INCREMENT)
	designer.set_payload_mass(new_payload)
	_update_header()

func _on_payload_increase_pressed():
	var current = designer.get_payload_mass()
	var new_payload = min(PAYLOAD_MAX, current + PAYLOAD_INCREMENT)
	designer.set_payload_mass(new_payload)
	_update_header()

func _on_back_button_pressed():
	back_requested.emit()

func _on_save_button_pressed():
	if game_manager:
		# Get the current design name
		var current_name = designer.get_design_name()

		# Create a simple save dialog
		var dialog = ConfirmationDialog.new()
		dialog.title = "Save Design"

		var vbox = VBoxContainer.new()
		dialog.add_child(vbox)

		var label = Label.new()
		label.text = "Enter a name for this design:"
		vbox.add_child(label)

		var name_input = LineEdit.new()
		name_input.text = current_name
		name_input.select_all_on_focus = true
		name_input.custom_minimum_size = Vector2(300, 0)
		vbox.add_child(name_input)

		add_child(dialog)
		dialog.popup_centered()
		name_input.grab_focus()

		# Handle confirm
		dialog.confirmed.connect(func():
			var new_name = name_input.text.strip_edges()
			if new_name.is_empty():
				new_name = "Unnamed Rocket"
			designer.set_design_name(new_name)
			# Sync design from designer to game state before saving
			game_manager.sync_design_from(designer)
			game_manager.ensure_design_saved(designer)
			design_saved.emit()
			dialog.queue_free()
			_show_save_notification(new_name)
		)

		dialog.canceled.connect(func():
			dialog.queue_free()
		)

func _show_save_notification(name: String):
	# Show a brief notification that the design was saved
	var notification = Label.new()
	notification.text = "Design '%s' saved!" % name
	notification.add_theme_font_size_override("font_size", 18)
	notification.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
	notification.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	notification.position = Vector2(get_viewport_rect().size.x / 2 - 150, 100)
	notification.custom_minimum_size = Vector2(300, 40)
	add_child(notification)

	# Fade out and remove after 2 seconds
	var tween = create_tween()
	tween.tween_property(notification, "modulate:a", 0.0, 0.5).set_delay(1.5)
	tween.tween_callback(notification.queue_free)

func set_game_manager(gm: GameManager):
	game_manager = gm
	if game_manager and not game_manager.is_connected("designs_changed", _on_designs_changed):
		game_manager.connect("designs_changed", _on_designs_changed)

func _on_designs_changed():
	# Sync fresh engine data from Company to Designer, then rebuild cards
	if game_manager:
		game_manager.sync_engines_to_designer(designer)
	_setup_engine_cards()

# Called by main scene to get the designer node
func get_designer() -> RocketDesigner:
	return designer
