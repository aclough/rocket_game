extends Control

## Missions content showing available contracts
## Allows selecting contracts and viewing active contract

signal contract_selected(contract_id: int)
signal design_requested

# Game manager reference (set by game shell)
var game_manager: GameManager = null

# UI references
@onready var contracts_list = $MarginContainer/VBox/ContractsPanel/ContractsMargin/ContractsScroll/ContractsList
@onready var refresh_button = $MarginContainer/VBox/TitlePanel/TitleMargin/TitleHBox/RefreshButton
@onready var active_contract_panel = $MarginContainer/VBox/ActiveContractPanel
@onready var active_name = $MarginContainer/VBox/ActiveContractPanel/ActiveMargin/ActiveHBox/ActiveVBox/ActiveName
@onready var active_details = $MarginContainer/VBox/ActiveContractPanel/ActiveMargin/ActiveHBox/ActiveVBox/ActiveDetails

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
		call_deferred("_update_ui")

func _update_ui():
	if not game_manager:
		return
	_update_contracts_list()
	_update_refresh_button()
	_update_active_contract()

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
	else:
		active_contract_panel.visible = false

func _on_contracts_changed():
	_update_ui()

func _on_money_changed(_new_amount: float):
	_update_refresh_button()

func _on_contract_select_pressed(contract_id: int):
	game_manager.select_contract(contract_id)
	contract_selected.emit(contract_id)

func _on_refresh_button_pressed():
	game_manager.refresh_contracts()

func _on_design_button_pressed():
	design_requested.emit()

func _on_abandon_button_pressed():
	game_manager.abandon_contract()
