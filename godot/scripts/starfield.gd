extends Control

# Simple starfield background with altitude-based visibility

@export var star_count: int = 150
@export var star_speed: float = 20.0

var stars: Array = []
var altitude: float = 0.0  # 0 = ground (no stars), 1 = space (full stars)

func _ready():
	generate_stars()

func set_altitude(value: float):
	altitude = clamp(value, 0.0, 1.0)

func generate_stars():
	stars.clear()
	for i in range(star_count):
		var star = {
			"position": Vector2(randf() * size.x, randf() * size.y),
			"size": randf_range(1.0, 3.0),
			"brightness": randf_range(0.3, 1.0),
			"twinkle_offset": randf() * TAU
		}
		stars.append(star)

func _process(delta):
	queue_redraw()

func _draw():
	if stars.is_empty() or altitude <= 0.0:
		return

	var time = Time.get_ticks_msec() / 1000.0

	for star in stars:
		# Twinkle effect
		var twinkle = 0.7 + 0.3 * sin(time * 2.0 + star.twinkle_offset)
		# Stars fade in based on altitude (atmosphere blocks them at low altitude)
		var alpha = star.brightness * twinkle * altitude

		var color = Color(1.0, 1.0, 1.0, alpha)
		draw_circle(star.position, star.size, color)

func _on_resized():
	generate_stars()
