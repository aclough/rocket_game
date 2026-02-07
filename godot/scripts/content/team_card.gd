extends PanelContainer

## Draggable card representing an engineering team
## Shows team name, status, and ramp-up state

signal team_clicked(team_id: int)

var team_id: int = -1
var team_name: String = ""
var is_ramping_up: bool = false
var ramp_up_days: int = 0
var is_assigned: bool = false
var assignment_type: String = ""  # "none", "design", "engine"
var assignment_index: int = -1

@onready var name_label = $MarginContainer/VBox/NameLabel
@onready var status_label = $MarginContainer/VBox/StatusLabel
@onready var ramp_up_bar = $MarginContainer/VBox/RampUpBar

func _ready():
	gui_input.connect(_on_gui_input)

func setup(id: int, data: Dictionary) -> void:
	team_id = id
	team_name = data.get("name", "Team " + str(id))
	is_ramping_up = data.get("ramping_up", false)
	ramp_up_days = data.get("ramp_up_days", 0)
	is_assigned = data.get("assigned", false)
	assignment_type = data.get("assignment_type", "none")
	assignment_index = data.get("assignment_index", -1)

	_update_display()

func _update_display() -> void:
	if name_label:
		name_label.text = team_name

	if status_label:
		if is_ramping_up:
			status_label.text = "Ramping up (%d days)" % ramp_up_days
			status_label.add_theme_color_override("font_color", Color(1.0, 0.6, 0.2))
		elif is_assigned:
			match assignment_type:
				"design":
					status_label.text = "Working on design"
				"engine":
					status_label.text = "Working on engine"
				_:
					status_label.text = "Assigned"
			status_label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
		else:
			status_label.text = "Available"
			status_label.add_theme_color_override("font_color", Color(0.5, 1.0, 0.5))

	if ramp_up_bar:
		ramp_up_bar.visible = is_ramping_up
		if is_ramping_up and ramp_up_days > 0:
			# Assuming 7 days total ramp-up
			ramp_up_bar.value = (7 - ramp_up_days) / 7.0 * 100.0

	# Orange tint when ramping up
	if is_ramping_up:
		modulate = Color(1.0, 0.8, 0.6)
	else:
		modulate = Color(1.0, 1.0, 1.0)

func _on_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		team_clicked.emit(team_id)

# Drag support
func _get_drag_data(_at_position: Vector2) -> Variant:
	# Create drag preview
	var preview = Label.new()
	preview.text = team_name
	preview.add_theme_color_override("font_color", Color(1.0, 1.0, 1.0))
	set_drag_preview(preview)

	return {
		"type": "team",
		"team_id": team_id,
		"team_name": team_name
	}
