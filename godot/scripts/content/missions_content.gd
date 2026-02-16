extends Control

## Missions content showing available contracts and depot missions
## Allows selecting contracts/depot missions and viewing active mission

signal contract_selected(contract_id: int)
signal depot_mission_selected
signal design_requested

# Game manager reference (set by game shell)
var game_manager: GameManager = null

# UI references
@onready var contracts_list = $MarginContainer/VBox/ContractsPanel/ContractsMargin/ContractsScroll/ContractsList
@onready var refresh_button = $MarginContainer/VBox/TitlePanel/TitleMargin/TitleHBox/RefreshButton
@onready var active_contract_panel = $MarginContainer/VBox/ActiveContractPanel
@onready var active_name = $MarginContainer/VBox/ActiveContractPanel/ActiveMargin/ActiveHBox/ActiveVBox/ActiveName
@onready var active_details = $MarginContainer/VBox/ActiveContractPanel/ActiveMargin/ActiveHBox/ActiveVBox/ActiveDetails

var _flights_container: VBoxContainer = null
var _company_missions_container: VBoxContainer = null
var _no_flights_label: Label = null

# Depot mission selection UI elements
var _depot_option: OptionButton = null
var _dest_option: OptionButton = null
var _depot_info_label: Label = null
var _depot_select_btn: Button = null

func _ready():
	pass

func set_game_manager(gm: GameManager):
	# Disconnect from old game manager if any
	if game_manager:
		if game_manager.contracts_changed.is_connected(_on_contracts_changed):
			game_manager.contracts_changed.disconnect(_on_contracts_changed)
		if game_manager.money_changed.is_connected(_on_money_changed):
			game_manager.money_changed.disconnect(_on_money_changed)

	game_manager = gm

	if game_manager:
		game_manager.contracts_changed.connect(_on_contracts_changed)
		game_manager.money_changed.connect(_on_money_changed)
		if game_manager.has_signal("flight_arrived"):
			game_manager.flight_arrived.connect(_on_flight_arrived_update)
		if game_manager.has_signal("inventory_changed"):
			game_manager.inventory_changed.connect(_on_inventory_changed)
		_setup_dynamic_sections()
		call_deferred("_update_ui")

func _setup_dynamic_sections():
	var vbox = $MarginContainer/VBox

	# === Active Flights Section ===
	var flights_panel = PanelContainer.new()
	flights_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	vbox.add_child(flights_panel)

	var flights_margin = MarginContainer.new()
	flights_margin.add_theme_constant_override("margin_left", 20)
	flights_margin.add_theme_constant_override("margin_top", 15)
	flights_margin.add_theme_constant_override("margin_right", 20)
	flights_margin.add_theme_constant_override("margin_bottom", 15)
	flights_panel.add_child(flights_margin)

	var flights_vbox = VBoxContainer.new()
	flights_vbox.add_theme_constant_override("separation", 8)
	flights_margin.add_child(flights_vbox)

	var flights_title = Label.new()
	flights_title.text = "ACTIVE FLIGHTS"
	flights_title.add_theme_font_size_override("font_size", 18)
	flights_vbox.add_child(flights_title)

	_no_flights_label = Label.new()
	_no_flights_label.text = "No active flights"
	_no_flights_label.add_theme_font_size_override("font_size", 13)
	_no_flights_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	flights_vbox.add_child(_no_flights_label)

	_flights_container = VBoxContainer.new()
	_flights_container.add_theme_constant_override("separation", 6)
	flights_vbox.add_child(_flights_container)

	# === Company Missions Section (Depot Selection UI) ===
	var company_panel = PanelContainer.new()
	company_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	vbox.add_child(company_panel)

	var company_margin = MarginContainer.new()
	company_margin.add_theme_constant_override("margin_left", 20)
	company_margin.add_theme_constant_override("margin_top", 15)
	company_margin.add_theme_constant_override("margin_right", 20)
	company_margin.add_theme_constant_override("margin_bottom", 15)
	company_panel.add_child(company_margin)

	_company_missions_container = VBoxContainer.new()
	_company_missions_container.add_theme_constant_override("separation", 10)
	company_margin.add_child(_company_missions_container)

	var company_title = Label.new()
	company_title.text = "DEPOT MISSIONS"
	company_title.add_theme_font_size_override("font_size", 18)
	_company_missions_container.add_child(company_title)

	# Depot selector row
	var depot_row = HBoxContainer.new()
	depot_row.add_theme_constant_override("separation", 10)
	_company_missions_container.add_child(depot_row)

	var depot_label = Label.new()
	depot_label.text = "Depot:"
	depot_label.add_theme_font_size_override("font_size", 14)
	depot_row.add_child(depot_label)

	_depot_option = OptionButton.new()
	_depot_option.add_theme_font_size_override("font_size", 13)
	_depot_option.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_depot_option.item_selected.connect(_on_depot_selection_changed)
	depot_row.add_child(_depot_option)

	# Destination selector row
	var dest_row = HBoxContainer.new()
	dest_row.add_theme_constant_override("separation", 10)
	_company_missions_container.add_child(dest_row)

	var dest_label = Label.new()
	dest_label.text = "Destination:"
	dest_label.add_theme_font_size_override("font_size", 14)
	dest_row.add_child(dest_label)

	_dest_option = OptionButton.new()
	_dest_option.add_theme_font_size_override("font_size", 13)
	_dest_option.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_dest_option.item_selected.connect(_on_depot_selection_changed)
	dest_row.add_child(_dest_option)

	# Info label
	_depot_info_label = Label.new()
	_depot_info_label.add_theme_font_size_override("font_size", 13)
	_depot_info_label.add_theme_color_override("font_color", Color(0.6, 0.8, 1.0))
	_company_missions_container.add_child(_depot_info_label)

	# Select mission button
	_depot_select_btn = Button.new()
	_depot_select_btn.text = "SELECT DEPOT MISSION"
	_depot_select_btn.add_theme_font_size_override("font_size", 14)
	_depot_select_btn.pressed.connect(_on_select_depot_mission_pressed)
	_company_missions_container.add_child(_depot_select_btn)

func _update_ui():
	if not game_manager:
		return
	_update_contracts_list()
	_update_refresh_button()
	_update_active_contract()
	_update_active_flights()
	_update_depot_selectors()

func _update_contracts_list():
	# Clear existing contracts
	for child in contracts_list.get_children():
		child.queue_free()

	# Add contract cards
	var count = game_manager.get_contract_count()
	for i in range(count):
		var card = _create_contract_card(i)
		contracts_list.add_child(card)

func _create_contract_card(index: int) -> PanelContainer:
	var contract_id = game_manager.get_contract_id(index)
	var cname = game_manager.get_contract_name(index)
	var destination = game_manager.get_contract_destination(index)
	var dest_short = game_manager.get_contract_destination_short(index)
	var delta_v = game_manager.get_contract_delta_v(index)
	var payload = game_manager.get_contract_payload(index)
	var reward = game_manager.get_contract_reward_formatted(index)

	var panel = PanelContainer.new()
	panel.custom_minimum_size = Vector2(0, 90)

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 20)
	margin.add_theme_constant_override("margin_right", 20)
	margin.add_theme_constant_override("margin_top", 12)
	margin.add_theme_constant_override("margin_bottom", 12)
	panel.add_child(margin)

	var hbox = HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 30)
	margin.add_child(hbox)

	# Left side: Contract info
	var info_vbox = VBoxContainer.new()
	info_vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	info_vbox.add_theme_constant_override("separation", 5)
	hbox.add_child(info_vbox)

	# Contract name
	var name_label = Label.new()
	name_label.text = cname
	name_label.add_theme_font_size_override("font_size", 18)
	info_vbox.add_child(name_label)

	# Details row
	var details_hbox = HBoxContainer.new()
	details_hbox.add_theme_constant_override("separation", 30)
	info_vbox.add_child(details_hbox)

	# Destination
	var dest_label = Label.new()
	dest_label.text = "%s (%s)" % [destination, dest_short]
	dest_label.add_theme_font_size_override("font_size", 14)
	dest_label.add_theme_color_override("font_color", _get_destination_color(dest_short))
	details_hbox.add_child(dest_label)

	# Delta-v
	var dv_label = Label.new()
	dv_label.text = "%.0f m/s" % delta_v
	dv_label.add_theme_font_size_override("font_size", 14)
	dv_label.add_theme_color_override("font_color", Color(0.6, 0.8, 1.0))
	details_hbox.add_child(dv_label)

	# Payload
	var payload_label = Label.new()
	payload_label.text = "%.0f kg" % payload
	payload_label.add_theme_font_size_override("font_size", 14)
	payload_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
	details_hbox.add_child(payload_label)

	# Right side: Reward and button
	var right_vbox = VBoxContainer.new()
	right_vbox.alignment = BoxContainer.ALIGNMENT_CENTER
	right_vbox.add_theme_constant_override("separation", 8)
	hbox.add_child(right_vbox)

	# Reward
	var reward_label = Label.new()
	reward_label.text = reward
	reward_label.add_theme_font_size_override("font_size", 22)
	reward_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
	reward_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	right_vbox.add_child(reward_label)

	# Select button
	var select_btn = Button.new()
	select_btn.text = "SELECT"
	select_btn.custom_minimum_size = Vector2(100, 35)
	select_btn.add_theme_font_size_override("font_size", 14)
	select_btn.pressed.connect(_on_contract_select_pressed.bind(contract_id))
	right_vbox.add_child(select_btn)

	return panel

func _get_destination_color(dest_short: String) -> Color:
	match dest_short:
		"SUB":
			return Color(0.5, 0.7, 0.5)
		"LEO":
			return Color(0.3, 0.8, 0.3)
		"SSO":
			return Color(0.5, 0.9, 0.5)
		"MEO":
			return Color(1.0, 1.0, 0.3)
		"GTO":
			return Color(1.0, 0.7, 0.3)
		"GEO":
			return Color(1.0, 0.4, 0.4)
		"LUNAR":
			return Color(0.8, 0.8, 1.0)
		_:
			return Color(0.7, 0.7, 0.7)

func _update_refresh_button():
	var cost = game_manager.get_refresh_cost_formatted()
	refresh_button.text = "REFRESH - " + cost
	refresh_button.disabled = not game_manager.can_refresh_contracts()

func _update_active_contract():
	if game_manager.has_active_contract():
		active_contract_panel.visible = true
		active_name.text = game_manager.get_active_contract_name()
		var dest = game_manager.get_active_contract_destination()
		var dv = game_manager.get_active_contract_delta_v()
		var reward = game_manager.get_active_contract_reward_formatted()
		active_details.text = "%s - %.0f m/s - %s" % [dest, dv, reward]
	elif game_manager.has_active_depot_mission():
		active_contract_panel.visible = true
		active_name.text = "DEPOT: " + game_manager.get_active_depot_mission_name()
		var dest = game_manager.get_active_depot_mission_destination()
		var dv = game_manager.get_active_depot_mission_delta_v()
		var payload = game_manager.get_active_depot_mission_payload()
		active_details.text = "%s - %.0f m/s - %.0f kg" % [dest, dv, payload]
	else:
		active_contract_panel.visible = false

func _update_depot_selectors():
	if not _depot_option or not _dest_option:
		return

	# Rebuild depot selector
	var prev_depot = _depot_option.selected
	_depot_option.clear()
	var depot_count = game_manager.get_depot_inventory_count()
	for i in range(depot_count):
		var dname = game_manager.get_depot_inventory_name(i)
		var serial = game_manager.get_depot_inventory_serial(i)
		var capacity = game_manager.get_depot_inventory_capacity(i)
		var mass = game_manager.get_depot_inventory_mass(i)
		_depot_option.add_item("%s (S/N %d) - %.0f kg cap, %.0f kg" % [dname, serial, capacity, mass])
	if prev_depot >= 0 and prev_depot < depot_count:
		_depot_option.selected = prev_depot

	# Rebuild destination selector (only if empty — destinations are static)
	if _dest_option.item_count == 0:
		var loc_count = game_manager.get_orbital_location_count()
		for i in range(loc_count):
			var lname = game_manager.get_orbital_location_name(i)
			var dv = game_manager.get_orbital_location_delta_v(i)
			_dest_option.add_item("%s (%.0f m/s)" % [lname, dv])

	# Update info and button state
	_update_depot_info()

func _update_depot_info():
	if not _depot_option or not _dest_option or not _depot_info_label:
		return

	var has_depot = _depot_option.item_count > 0 and _depot_option.selected >= 0
	var has_dest = _dest_option.item_count > 0 and _dest_option.selected >= 0

	if has_depot and has_dest:
		var depot_idx = _depot_option.selected
		var dest_idx = _dest_option.selected
		var mass = game_manager.get_depot_inventory_mass(depot_idx)
		var location_id = game_manager.get_orbital_location_id(dest_idx)
		var transit = game_manager.get_mission_transit_days(location_id)
		var dv = game_manager.get_orbital_location_delta_v(dest_idx)
		_depot_info_label.text = "Mass: %.0f kg | Δv: %.0f m/s | Transit: %d day%s" % [
			mass, dv, transit, "s" if transit != 1 else ""]
		_depot_select_btn.disabled = false
	elif _depot_option.item_count == 0:
		_depot_info_label.text = "No depots in inventory. Build one first."
		_depot_select_btn.disabled = true
	else:
		_depot_info_label.text = ""
		_depot_select_btn.disabled = true

func _on_depot_selection_changed(_index: int):
	_update_depot_info()

func _on_select_depot_mission_pressed():
	if not _depot_option or not _dest_option:
		return
	var depot_idx = _depot_option.selected
	var dest_idx = _dest_option.selected
	if depot_idx < 0 or dest_idx < 0:
		return

	var depot_serial = game_manager.get_depot_inventory_serial(depot_idx)
	var location_id = game_manager.get_orbital_location_id(dest_idx)

	if game_manager.select_depot_mission(depot_serial, location_id):
		depot_mission_selected.emit()
	else:
		_show_toast("Failed to select depot mission")

func _on_contracts_changed():
	_update_ui()

func _on_money_changed(_new_amount: float):
	_update_refresh_button()

func _on_inventory_changed():
	_update_depot_selectors()

func _on_contract_select_pressed(contract_id: int):
	game_manager.select_contract(contract_id)
	contract_selected.emit(contract_id)

func _on_refresh_button_pressed():
	game_manager.refresh_contracts()

func _on_design_button_pressed():
	design_requested.emit()

func _on_abandon_button_pressed():
	if game_manager.has_active_depot_mission():
		game_manager.cancel_depot_mission()
	else:
		game_manager.abandon_contract()

func _update_active_flights():
	if not _flights_container:
		return

	for child in _flights_container.get_children():
		child.queue_free()

	var count = game_manager.get_active_flight_count()
	_no_flights_label.visible = (count == 0)

	for i in range(count):
		var dest = game_manager.get_active_flight_destination(i)
		var days = game_manager.get_active_flight_days_remaining(i)
		var payload_type = game_manager.get_active_flight_payload_type(i)

		var hbox = HBoxContainer.new()
		hbox.add_theme_constant_override("separation", 15)
		_flights_container.add_child(hbox)

		var type_label = Label.new()
		if payload_type == "depot":
			type_label.text = "Depot"
			type_label.add_theme_color_override("font_color", Color(0.9, 0.7, 0.4))
		else:
			type_label.text = "Contract"
			type_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
		type_label.add_theme_font_size_override("font_size", 14)
		hbox.add_child(type_label)

		var dest_label = Label.new()
		dest_label.text = dest
		dest_label.add_theme_font_size_override("font_size", 14)
		dest_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		hbox.add_child(dest_label)

		var days_label = Label.new()
		if days == 0:
			days_label.text = "Arriving..."
			days_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
		else:
			days_label.text = "%d day%s remaining" % [days, "s" if days != 1 else ""]
			days_label.add_theme_color_override("font_color", Color(0.6, 0.8, 1.0))
		days_label.add_theme_font_size_override("font_size", 14)
		hbox.add_child(days_label)

func _on_flight_arrived_update(_flight_id: int, _destination: String, _reward: float):
	_update_active_flights()

func _show_toast(message: String):
	# Propagate toast to parent (game_shell handles toasts)
	var parent = get_tree().root.get_node_or_null("GameShell")
	if parent and parent.has_method("_show_toast"):
		parent._show_toast(message)
