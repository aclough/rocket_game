extends Control

@onready var rocket = $Rocket

var is_launching = false
var launch_progress = 0.0
var launch_speed = 0.5

func _ready():
	reset_position()

func _process(delta):
	if is_launching:
		launch_progress += delta * launch_speed
		rocket.position.y = lerp(200.0, -100.0, launch_progress)

		# Add slight rotation for effect
		rocket.rotation = sin(launch_progress * 10.0) * 0.05

func start_launch():
	is_launching = true
	launch_progress = 0.0
	rocket.visible = true

func stop_launch():
	is_launching = false

func reset_position():
	is_launching = false
	launch_progress = 0.0
	rocket.position = Vector2(0, 200)
	rocket.rotation = 0
	rocket.visible = false
