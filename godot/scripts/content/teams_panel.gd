extends PanelContainer

## Panel showing engineering teams
## Allows hiring teams and displays team status

signal team_selected(team_id: int)

const TeamCard = preload("res://scenes/content/team_card.tscn")

var game_manager: GameManager = null

@onready var teams_container = $MarginContainer/VBox/ScrollContainer/TeamsContainer
@onready var hire_button = $MarginContainer/VBox/Header/HireButton
@onready var salary_label = $MarginContainer/VBox/Header/SalaryLabel
@onready var team_count_label = $MarginContainer/VBox/Header/TeamCountLabel

func _ready():
	if hire_button:
		hire_button.pressed.connect(_on_hire_pressed)

func set_game_manager(gm: GameManager) -> void:
	game_manager = gm
	if game_manager:
		game_manager.teams_changed.connect(_on_teams_changed)
		_update_ui()

func _update_ui() -> void:
	if not game_manager:
		return

	# Update team count
	var count = game_manager.get_team_count()
	if team_count_label:
		team_count_label.text = "Teams: %d" % count

	# Update salary display
	if salary_label:
		salary_label.text = "Salary: %s/mo" % game_manager.get_total_monthly_salary_formatted()

	# Clear and rebuild team cards
	if teams_container:
		for child in teams_container.get_children():
			child.queue_free()

		var team_ids = game_manager.get_all_team_ids()
		for id in team_ids:
			var card = TeamCard.instantiate()
			teams_container.add_child(card)

			var assignment = game_manager.get_team_assignment(id)

			card.setup(id, {
				"name": game_manager.get_team_name(id),
				"ramping_up": game_manager.is_team_ramping_up(id),
				"ramp_up_days": game_manager.get_team_ramp_up_days(id),
				"assigned": game_manager.is_team_assigned(id),
				"assignment_type": assignment.get("type", "none"),
				"assignment_index": assignment.get("design_index", -1)
			})

			card.team_clicked.connect(_on_team_clicked)

func _on_hire_pressed() -> void:
	if game_manager:
		game_manager.hire_team()

func _on_teams_changed() -> void:
	_update_ui()

func _on_team_clicked(team_id: int) -> void:
	team_selected.emit(team_id)
