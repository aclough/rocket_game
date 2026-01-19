extends ColorRect

# Screen effects for dramatic moments

func _ready():
	modulate = Color(1, 1, 1, 0)  # Start transparent

func flash_explosion():
	# Red flash for explosion
	modulate = Color(1.0, 0.3, 0.0, 0.5)

	var tween = create_tween()
	tween.tween_property(self, "modulate:a", 0.0, 0.5)

func flash_success():
	# Green flash for success
	modulate = Color(0.3, 1.0, 0.3, 0.3)

	var tween = create_tween()
	tween.tween_property(self, "modulate:a", 0.0, 0.8)

func screen_shake(intensity: float = 10.0, duration: float = 0.3):
	var parent = get_parent()
	if not parent:
		return

	var original_position = parent.position
	var shake_timer = 0.0

	while shake_timer < duration:
		var offset = Vector2(
			randf_range(-intensity, intensity),
			randf_range(-intensity, intensity)
		)
		parent.position = original_position + offset

		await get_tree().create_timer(0.05).timeout
		shake_timer += 0.05

	parent.position = original_position
