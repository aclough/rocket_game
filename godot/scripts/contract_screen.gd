extends Control

signal contract_selected(contract_id: int)
signal free_launch_requested
signal new_game_requested

# Game manager reference
@onready var game_manager: GameManager = $GameManager

# UI references
@onready var money_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderHBox/StatsVBox/MoneyLabel
@onready var turn_label = $MarginContainer/VBox/HeaderPanel/HeaderMargin/HeaderHBox/StatsVBox/TurnLabel
@onready var contracts_list = $MarginContainer/VBox/ContractsPanel/ContractsMargin/ContractsVBox/ContractsScroll/ContractsList
@onready var refresh_button = $MarginContainer/VBox/ContractsPanel/ContractsMargin/ContractsVBox/ContractsHeader/RefreshButton

func _ready():
	# Connect game manager signals
	game_manager.money_changed.connect(_on_money_changed)
	game_manager.contracts_changed.connect(_on_contracts_changed)

	# Initial UI update
	call_deferred("_update_ui")

func _update_ui():
	_update_header()
	_update_contracts_list()
	_update_refresh_button()

func _update_header():
	money_label.text = "Funds: " + game_manager.get_money_formatted()

	var turn = game_manager.get_turn()
	var successes = game_manager.get_successful_launches()
	var total = game_manager.get_total_launches()

	if total > 0:
		turn_label.text = "Turn %d | Launches: %d/%d (%.0f%%)" % [turn, successes, total, game_manager.get_success_rate()]
	else:
		turn_label.text = "Turn %d | No launches yet" % turn

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
	var name = game_manager.get_contract_name(index)
	var destination = game_manager.get_contract_destination(index)
	var dest_short = game_manager.get_contract_destination_short(index)
	var delta_v = game_manager.get_contract_delta_v(index)
	var payload = game_manager.get_contract_payload(index)
	var reward = game_manager.get_contract_reward_formatted(index)

	var panel = PanelContainer.new()
	panel.custom_minimum_size = Vector2(0, 100)

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 20)
	margin.add_theme_constant_override("margin_right", 20)
	margin.add_theme_constant_override("margin_top", 15)
	margin.add_theme_constant_override("margin_bottom", 15)
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
	name_label.text = name
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
	right_vbox.add_theme_constant_override("separation", 10)
	hbox.add_child(right_vbox)

	# Reward
	var reward_label = Label.new()
	reward_label.text = reward
	reward_label.add_theme_font_size_override("font_size", 24)
	reward_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
	reward_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	right_vbox.add_child(reward_label)

	# Select button
	var select_btn = Button.new()
	select_btn.text = "SELECT"
	select_btn.custom_minimum_size = Vector2(120, 40)
	select_btn.add_theme_font_size_override("font_size", 16)
	select_btn.pressed.connect(_on_contract_select_pressed.bind(contract_id))
	right_vbox.add_child(select_btn)

	return panel

func _get_destination_color(dest_short: String) -> Color:
	match dest_short:
		"SUB":
			return Color(0.5, 0.7, 0.5)  # Light green - easy
		"LEO":
			return Color(0.3, 0.8, 0.3)  # Green - easy
		"SSO":
			return Color(0.5, 0.9, 0.5)  # Brighter green
		"MEO":
			return Color(1.0, 1.0, 0.3)  # Yellow - medium
		"GTO":
			return Color(1.0, 0.7, 0.3)  # Orange - harder
		"GEO":
			return Color(1.0, 0.4, 0.4)  # Red - hardest
		_:
			return Color(0.7, 0.7, 0.7)

func _update_refresh_button():
	var cost = game_manager.get_refresh_cost_formatted()
	refresh_button.text = "REFRESH - " + cost
	refresh_button.disabled = not game_manager.can_refresh_contracts()

func _on_money_changed(_new_amount: float):
	_update_header()
	_update_refresh_button()

func _on_contracts_changed():
	_update_contracts_list()

func _on_contract_select_pressed(contract_id: int):
	game_manager.select_contract(contract_id)
	contract_selected.emit(contract_id)

func _on_refresh_button_pressed():
	game_manager.refresh_contracts()

func _on_free_launch_pressed():
	free_launch_requested.emit()

func _on_new_game_pressed():
	game_manager.new_game()
	new_game_requested.emit()

# Called by main scene to get the game manager
func get_game_manager() -> GameManager:
	return game_manager
