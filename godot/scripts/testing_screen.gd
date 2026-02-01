extends Control

# Designer node reference (passed from design screen)
var designer: RocketDesigner = null

# UI references - Header
@onready var design_name_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderVBox/DesignInfo/DesignName

# UI references - Tests panel (will hold dynamic engine test cards)
@onready var tests_vbox = $MarginContainer/VBox/ContentHBox/TestsPanel/TestsMargin/TestsVBox
@onready var engine_tests_container = $MarginContainer/VBox/ContentHBox/TestsPanel/TestsMargin/TestsVBox/EngineTestsContainer
@onready var rocket_test_button = $MarginContainer/VBox/ContentHBox/TestsPanel/TestsMargin/TestsVBox/RocketTestPanel/RocketTestMargin/RocketTestVBox/RocketTestButton
@onready var rocket_test_cost_label = $MarginContainer/VBox/ContentHBox/TestsPanel/TestsMargin/TestsVBox/RocketTestPanel/RocketTestMargin/RocketTestVBox/RocketTestCost

# UI references - Stats
@onready var unknown_label = $MarginContainer/VBox/ContentHBox/TestsPanel/TestsMargin/TestsVBox/StatsPanel/StatsMargin/StatsVBox/UnknownLabel
@onready var success_label = $MarginContainer/VBox/ContentHBox/TestsPanel/TestsMargin/TestsVBox/StatsPanel/StatsMargin/StatsVBox/SuccessLabel

# UI references - Flaws
@onready var flaws_list = $MarginContainer/VBox/ContentHBox/FlawsPanel/FlawsMargin/FlawsVBox/FlawsScroll/FlawsList
@onready var test_result_label = $MarginContainer/VBox/ContentHBox/FlawsPanel/FlawsMargin/FlawsVBox/TestResultLabel

# Track dynamically created engine test buttons
var engine_test_buttons: Dictionary = {}

func _ready():
	# Initial UI update will happen when designer is set
	pass

# Called by main.gd to set the designer reference
func set_designer(d: RocketDesigner):
	designer = d
	if designer:
		# Ensure flaws are generated
		designer.ensure_flaws_generated()
		# Connect to design changes
		if not designer.design_changed.is_connected(_on_design_changed):
			designer.design_changed.connect(_on_design_changed)
		# Defer the UI update to ensure @onready vars are initialized
		call_deferred("_update_ui")

func _on_design_changed():
	_update_ui()

func _update_ui():
	if not designer:
		return

	# Safety check - ensure @onready nodes are ready
	if not is_node_ready():
		call_deferred("_update_ui")
		return

	# Update header
	design_name_label.text = "Design: " + designer.get_design_name()

	# Rebuild engine test cards for each engine type
	_rebuild_engine_test_cards()

	# Update rocket test costs
	rocket_test_cost_label.text = "Cost: $" + _format_money(designer.get_rocket_test_cost())
	rocket_test_button.disabled = not designer.can_afford_rocket_test()

	# Update stats
	var flaw_range = designer.get_estimated_unknown_flaw_range()
	if flaw_range.size() >= 2:
		var min_flaws = flaw_range[0]
		var max_flaws = flaw_range[1]
		if min_flaws == max_flaws:
			unknown_label.text = "Unknown issues: ~%d" % min_flaws
		else:
			unknown_label.text = "Unknown issues: ~%d-%d" % [min_flaws, max_flaws]

	var success_rate = designer.get_estimated_success_rate() * 100
	success_label.text = "Est. success rate: %.0f%%" % success_rate

	# Color-code success rate
	if success_rate >= 70:
		success_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
	elif success_rate >= 40:
		success_label.add_theme_color_override("font_color", Color(1.0, 1.0, 0.3))
	else:
		success_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))

	# Update flaws list
	_rebuild_flaws_list()

func _rebuild_engine_test_cards():
	# Clear existing engine test cards
	if engine_tests_container:
		for child in engine_tests_container.get_children():
			child.queue_free()
	engine_test_buttons.clear()

	if not designer or not engine_tests_container:
		return

	# Get unique engine types in the design
	var engine_types = designer.get_unique_engine_types()
	var test_cost = designer.get_engine_test_cost()
	var can_afford = designer.can_afford_engine_test()

	for engine_type in engine_types:
		var engine_name = designer.get_engine_name(engine_type)
		var card = _create_engine_test_card(engine_type, engine_name, test_cost, can_afford)
		engine_tests_container.add_child(card)

func _create_engine_test_card(engine_type: int, engine_name: String, cost: float, can_afford: bool) -> PanelContainer:
	var panel = PanelContainer.new()

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 15)
	margin.add_theme_constant_override("margin_right", 15)
	margin.add_theme_constant_override("margin_top", 15)
	margin.add_theme_constant_override("margin_bottom", 15)
	panel.add_child(margin)

	var vbox = VBoxContainer.new()
	vbox.add_theme_constant_override("separation", 10)
	margin.add_child(vbox)

	# Title
	var title = Label.new()
	title.text = "%s Engine Test" % engine_name
	title.add_theme_font_size_override("font_size", 18)
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	vbox.add_child(title)

	# Description
	var desc = Label.new()
	desc.text = "Tests %s engine components for defects" % engine_name
	desc.add_theme_font_size_override("font_size", 12)
	desc.add_theme_color_override("font_color", Color(0.6, 0.6, 0.6))
	desc.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	vbox.add_child(desc)

	# Cost
	var cost_label = Label.new()
	cost_label.text = "Cost: $" + _format_money(cost)
	cost_label.add_theme_font_size_override("font_size", 14)
	cost_label.add_theme_color_override("font_color", Color(1, 0.85, 0.3))
	cost_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	vbox.add_child(cost_label)

	# Button
	var button = Button.new()
	button.text = "RUN %s TEST" % engine_name.to_upper()
	button.add_theme_font_size_override("font_size", 16)
	button.custom_minimum_size = Vector2(0, 40)
	button.disabled = not can_afford
	button.pressed.connect(_on_engine_test_for_type_pressed.bind(engine_type))
	vbox.add_child(button)

	engine_test_buttons[engine_type] = button

	return panel

func _rebuild_flaws_list():
	# Safety check
	if not is_node_ready() or flaws_list == null:
		return

	# Clear existing
	for child in flaws_list.get_children():
		child.queue_free()

	if not designer:
		return

	var flaw_count = designer.get_flaw_count()
	var any_discovered = false

	for i in range(flaw_count):
		var discovered = designer.is_flaw_discovered(i)
		if not discovered:
			continue

		any_discovered = true
		var fixed = designer.is_flaw_fixed(i)
		var name = designer.get_flaw_name(i)
		var is_engine = designer.is_flaw_engine_type(i)

		var card = _create_flaw_card(i, name, is_engine, fixed)
		flaws_list.add_child(card)

	if not any_discovered:
		var label = Label.new()
		label.text = "No issues discovered yet.\nRun tests to find hidden flaws."
		label.add_theme_font_size_override("font_size", 14)
		label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		flaws_list.add_child(label)

func _create_flaw_card(index: int, flaw_name: String, is_engine: bool, is_fixed: bool) -> PanelContainer:
	var panel = PanelContainer.new()

	if is_fixed:
		var style = StyleBoxFlat.new()
		style.set_bg_color(Color(0.08, 0.12, 0.08))
		style.set_border_width_all(1)
		style.set_border_color(Color(0.3, 0.6, 0.3, 0.5))
		panel.add_theme_stylebox_override("panel", style)

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 10)
	margin.add_theme_constant_override("margin_right", 10)
	margin.add_theme_constant_override("margin_top", 8)
	margin.add_theme_constant_override("margin_bottom", 8)
	panel.add_child(margin)

	var hbox = HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 10)
	margin.add_child(hbox)

	# Status icon
	var icon_label = Label.new()
	if is_fixed:
		icon_label.text = "[OK]"
		icon_label.add_theme_color_override("font_color", Color(0.3, 0.8, 0.3))
	else:
		icon_label.text = "[!!]"
		icon_label.add_theme_color_override("font_color", Color(1.0, 0.8, 0.2))
	icon_label.add_theme_font_size_override("font_size", 14)
	hbox.add_child(icon_label)

	# Flaw name and type
	var name_label = Label.new()
	var type_str = "(Design)"
	if is_engine:
		# Get the specific engine type name for this flaw
		var engine_type_idx = designer.get_flaw_engine_type_index(index)
		if engine_type_idx >= 0:
			var engine_name = designer.get_engine_name(engine_type_idx)
			type_str = "(%s)" % engine_name
		else:
			type_str = "(Engine)"
	name_label.text = flaw_name + " " + type_str
	name_label.add_theme_font_size_override("font_size", 14)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	if is_fixed:
		name_label.add_theme_color_override("font_color", Color(0.5, 0.6, 0.5))
	hbox.add_child(name_label)

	# Fix button (only if not fixed)
	if not is_fixed:
		var fix_button = Button.new()
		fix_button.text = "FIX $%s" % _format_money(designer.get_flaw_fix_cost())
		fix_button.add_theme_font_size_override("font_size", 12)
		fix_button.disabled = not designer.can_afford_fix()
		fix_button.pressed.connect(_on_fix_flaw_pressed.bind(index))
		hbox.add_child(fix_button)
	else:
		var fixed_label = Label.new()
		fixed_label.text = "FIXED"
		fixed_label.add_theme_font_size_override("font_size", 12)
		fixed_label.add_theme_color_override("font_color", Color(0.3, 0.6, 0.3))
		hbox.add_child(fixed_label)

	return panel

func _on_fix_flaw_pressed(index: int):
	if designer and designer.fix_flaw(index):
		test_result_label.text = "Issue fixed successfully."
		test_result_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))

func _on_engine_test_for_type_pressed(engine_type: int):
	if not designer:
		return

	var engine_name = designer.get_engine_name(engine_type)
	var discovered = designer.run_engine_test_for_type(engine_type)

	if discovered.size() > 0:
		test_result_label.text = "%s engine test discovered %d issue(s)!" % [engine_name, discovered.size()]
		test_result_label.add_theme_color_override("font_color", Color(1.0, 0.8, 0.2))
	else:
		test_result_label.text = "%s engine test complete. No new issues found." % engine_name
		test_result_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))

func _on_rocket_test_pressed():
	if not designer:
		return

	var discovered = designer.run_rocket_test()

	if discovered.size() > 0:
		test_result_label.text = "Rocket test discovered %d issue(s)!" % discovered.size()
		test_result_label.add_theme_color_override("font_color", Color(1.0, 0.8, 0.2))
	else:
		test_result_label.text = "Rocket test complete. No new issues found."
		test_result_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))

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
