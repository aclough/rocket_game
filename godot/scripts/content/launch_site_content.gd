extends Control

## Launch site content showing infrastructure and testing
## Includes pad upgrade, propellant storage, and testing panel

signal launch_requested

var game_manager: GameManager = null
var designer: RocketDesigner = null

# Infrastructure UI
@onready var pad_level_label = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/PadSection/PadLevelLabel
@onready var max_mass_label = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/PadSection/MaxMassLabel
@onready var upgrade_button = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/PadSection/UpgradeButton
@onready var storage_label = $MarginContainer/HBox/InfrastructurePanel/InfraMargin/InfraVBox/PropellantSection/StorageLabel
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

	_update_launch_readiness()

func _update_launch_readiness():
	if not game_manager or not designer:
		status_label.text = "No design loaded"
		status_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		launch_button.disabled = true
		return

	# Check if design data is actually loaded (not just an empty designer)
	if not designer.is_design_valid():
		status_label.text = "No design loaded"
		status_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		launch_button.disabled = true
		return

	var can_launch = game_manager.can_launch_current_rocket()
	var is_launchable = designer.is_launchable()

	if not is_launchable:
		# Provide more specific feedback about why
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
	elif not game_manager.has_rocket_for_current_design():
		status_label.text = "No manufactured rocket available"
		status_label.add_theme_color_override("font_color", Color(1.0, 0.5, 0.3))
		launch_button.disabled = true
	elif not can_launch:
		status_label.text = "Rocket too heavy for pad"
		status_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
		launch_button.disabled = true
	else:
		var testing_level = designer.get_testing_level()
		var testing_level_name = designer.get_testing_level_name()
		status_label.text = "Ready (%s)" % testing_level_name
		var level_color = _testing_level_color(testing_level)
		status_label.add_theme_color_override("font_color", level_color)
		launch_button.disabled = false

func _on_money_changed(_amount: float):
	_update_infrastructure()

func _on_inventory_changed():
	_update_launch_readiness()

func _on_upgrade_pressed():
	if game_manager and game_manager.upgrade_pad():
		_update_infrastructure()

func _on_launch_pressed():
	launch_requested.emit()

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
		_update_launch_readiness()
		# Also refresh the testing view to show any newly discovered flaws
		if testing_view:
			testing_view._update_ui()
