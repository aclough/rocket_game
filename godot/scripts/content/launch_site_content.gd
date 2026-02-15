extends Control

## Launch site content showing infrastructure and testing
## Includes pad upgrade, propellant storage, and testing panel

signal launch_requested(serial_number: int)

var game_manager: GameManager = null
var designer: RocketDesigner = null

# Currently selected rocket serial number
var _selected_serial: int = -1

# Infrastructure UI
@onready var pad_level_label = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/PadSection/PadLevelLabel
@onready var max_mass_label = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/PadSection/MaxMassLabel
@onready var upgrade_button = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/PadSection/UpgradeButton
@onready var storage_label = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/PropellantSection/StorageLabel
@onready var rocket_select = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/LaunchReadiness/RocketSelect
@onready var status_label = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/LaunchReadiness/StatusLabel
@onready var launch_button = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/LaunchReadiness/LaunchButton

# Testing view
@onready var testing_view = $MarginContainer/HBox/TestingView

func _ready():
	pass

func set_game_manager(gm: GameManager):
	game_manager = gm
	if game_manager:
		game_manager.money_changed.connect(_on_money_changed)
		game_manager.inventory_changed.connect(_on_inventory_changed)
		rocket_select.item_selected.connect(_on_rocket_selected)
		call_deferred("_update_infrastructure")

func set_designer(d: RocketDesigner):
	designer = d
	testing_view.set_designer(d)
	# Connect to design changes so we update when the design data is populated
	if designer and not designer.design_changed.is_connected(_on_design_changed):
		designer.design_changed.connect(_on_design_changed)
	call_deferred("_update_launch_readiness")

func _on_design_changed():
	_update_launch_readiness()

func _update_infrastructure():
	if not game_manager:
		return

	# Update pad info
	pad_level_label.text = "Level: " + game_manager.get_pad_level_name()
	max_mass_label.text = "Max Mass: " + game_manager.get_max_launch_mass_formatted()

	# Update upgrade button
	var upgrade_cost = game_manager.get_pad_upgrade_cost()
	if upgrade_cost > 0:
		upgrade_button.text = "UPGRADE - " + game_manager.get_pad_upgrade_cost_formatted()
		upgrade_button.disabled = not game_manager.can_upgrade_pad()
	else:
		upgrade_button.text = "MAX LEVEL"
		upgrade_button.disabled = true

	# Update propellant storage
	var storage = game_manager.get_propellant_storage()
	storage_label.text = "Capacity: %s kg" % _format_number(storage)

	_populate_rocket_select()
	_update_launch_readiness()

func _populate_rocket_select():
	if not game_manager or not rocket_select:
		return

	rocket_select.clear()
	_selected_serial = -1

	var inventory = game_manager.get_rocket_inventory()
	if inventory.size() == 0:
		rocket_select.add_item("No rockets manufactured")
		rocket_select.set_item_metadata(0, -1)
		rocket_select.disabled = true
		return

	rocket_select.disabled = false
	for i in range(inventory.size()):
		var entry = inventory[i]
		var rocket_name = entry.get("name", "Unknown")
		var serial = entry.get("serial_number", -1)
		var mass_kg = entry.get("mass_kg", 0.0)
		var mass_t = mass_kg / 1000.0
		rocket_select.add_item("S/N %d: %s (%.0ft)" % [serial, rocket_name, mass_t])
		rocket_select.set_item_metadata(i, serial)

	# Auto-select first rocket
	if inventory.size() > 0:
		rocket_select.selected = 0
		_selected_serial = rocket_select.get_item_metadata(0)
		# Set the design for this rocket so designer/contract logic works
		game_manager.select_rocket_for_launch(_selected_serial)

func _on_rocket_selected(index: int):
	if index < 0 or not rocket_select:
		_selected_serial = -1
		return
	_selected_serial = rocket_select.get_item_metadata(index)
	if game_manager and _selected_serial >= 0:
		game_manager.select_rocket_for_launch(_selected_serial)
	_update_launch_readiness()

func _update_launch_readiness():
	if not game_manager:
		status_label.text = "No rocket selected"
		status_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		launch_button.disabled = true
		return

	if _selected_serial < 0 or not game_manager.has_any_rocket_in_inventory():
		status_label.text = "No manufactured rocket available"
		status_label.add_theme_color_override("font_color", Color(1.0, 0.5, 0.3))
		launch_button.disabled = true
		return

	if not game_manager.can_launch_rocket_by_serial(_selected_serial):
		status_label.text = "Rocket too heavy for pad"
		status_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
		launch_button.disabled = true
		return

	# Check mission requirements if designer is available and has a contract
	if designer and designer.is_design_valid() and game_manager.has_active_contract():
		var is_launchable = designer.is_launchable()
		if not is_launchable:
			var is_sufficient = designer.is_design_sufficient()
			var is_within_budget = designer.is_within_budget()
			if not is_sufficient:
				var current_dv = designer.get_total_effective_delta_v()
				var target_dv = designer.get_target_delta_v()
				status_label.text = "Insufficient Δv: %.0f / %.0f m/s" % [current_dv, target_dv]
			elif not is_within_budget:
				status_label.text = "Over budget"
			else:
				status_label.text = "Design not ready"
			status_label.add_theme_color_override("font_color", Color(1.0, 0.5, 0.3))
			launch_button.disabled = true
			return

	# Ready to launch
	var testing_level_name = ""
	if designer and designer.is_design_valid():
		var testing_level = designer.get_testing_level()
		testing_level_name = designer.get_testing_level_name()
		status_label.text = "Ready (%s)" % testing_level_name
		status_label.add_theme_color_override("font_color", _testing_level_color(testing_level))
	else:
		status_label.text = "Ready"
		status_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
	launch_button.disabled = false

func _on_money_changed(_amount: float):
	_update_infrastructure()

func _on_inventory_changed():
	_populate_rocket_select()
	_update_launch_readiness()

func _on_upgrade_pressed():
	if game_manager and game_manager.upgrade_pad():
		_update_infrastructure()

func _on_launch_pressed():
	launch_requested.emit(_selected_serial)

# Explicitly refresh the testing view (called after launch returns)
func refresh_testing_view():
	if testing_view:
		testing_view._update_ui()

# Helper to get color for a testing level index (0-4)
func _testing_level_color(level: int) -> Color:
	match level:
		0: return Color(1.0, 0.3, 0.3)       # Untested - Red
		1: return Color(1.0, 0.6, 0.2)       # Lightly Tested - Orange
		2: return Color(1.0, 1.0, 0.3)       # Moderately Tested - Yellow
		3: return Color(0.6, 1.0, 0.4)       # Well Tested - Light green
		4: return Color(0.3, 1.0, 0.3)       # Thoroughly Tested - Green
		_: return Color(0.5, 0.5, 0.5)

func _format_number(value: float) -> String:
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

# Called when becoming visible
func _notification(what):
	if what == NOTIFICATION_VISIBILITY_CHANGED and visible:
		_update_infrastructure()
		# Also refresh the testing view to show any newly discovered flaws
		if testing_view:
			testing_view._update_ui()
