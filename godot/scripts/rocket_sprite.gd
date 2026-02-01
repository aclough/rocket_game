extends Control

@onready var rocket_image = $RocketImage
@onready var explosion_label = $ExplosionLabel
@onready var particles_manager = $ParticleEffects

# Animation state
var is_launching = false
var current_stage = 0
var total_stages = 7

# Gravity turn animation - rocket pitches from vertical to horizontal
# 0 radians = vertical (pointing up), PI/2 = horizontal (orbiting parallel to Earth)
const LAUNCH_ROTATION = 0.0  # Start vertical
const ORBIT_ROTATION = PI / 2.0  # End horizontal (90 degrees)

# Continuous animation progress (0 to 1)
var animation_progress: float = 0.0

# Particle effect references
var engine_particles: CPUParticles2D = null
var success_particles: CPUParticles2D = null

# Animation settings
const EXPLOSION_DURATION = 1.0
const SUCCESS_DURATION = 1.5
const ANIMATION_SPEED = 0.15  # How fast position/rotation progress per second
const MAX_Y_OFFSET = -200.0  # How far up the rocket travels during launch

func _ready():
	reset()

func _process(delta):
	if not is_launching or not rocket_image:
		return

	# Continuously advance animation progress
	animation_progress = min(animation_progress + delta * ANIMATION_SPEED, 1.0)

	# Position: move upward with slight easing
	var position_progress = ease(animation_progress, 0.8)
	rocket_image.position.y = MAX_Y_OFFSET * position_progress

	# Rotation: gravity turn with different easing (more gradual at start)
	var pitch_progress = ease(animation_progress, 0.5)
	var target_rotation = lerp(LAUNCH_ROTATION, ORBIT_ROTATION, pitch_progress)

	# Add subtle wobble for realism
	var wobble = sin(Time.get_ticks_msec() * 0.01) * 0.015
	rocket_image.rotation = target_rotation + wobble

func reset():
	is_launching = false
	current_stage = 0
	animation_progress = 0.0

	# Clean up particles
	if engine_particles:
		particles_manager.stop_engine_flame(engine_particles)
		engine_particles = null

	if success_particles:
		particles_manager.stop_success_sparkles(success_particles)
		success_particles = null

	if rocket_image:
		rocket_image.position = Vector2(0, 0)
		rocket_image.rotation = 0
		rocket_image.modulate = Color.WHITE
		rocket_image.visible = false

	if explosion_label:
		explosion_label.visible = false

func set_total_stages(count: int):
	total_stages = max(1, count)

func start_launch():
	is_launching = true
	current_stage = 0
	animation_progress = 0.0

	if rocket_image:
		rocket_image.visible = true
		rocket_image.position = Vector2(0, 0)
		rocket_image.rotation = LAUNCH_ROTATION  # Start vertical
		rocket_image.modulate = Color.WHITE

	# Start engine flame particles
	if particles_manager:
		engine_particles = particles_manager.create_engine_flame(Vector2(0, 64))

func advance_stage():
	if not is_launching:
		return
	current_stage += 1
	# Position and rotation are now handled continuously in _process()

func show_explosion():
	is_launching = false

	# Stop engine flames
	if engine_particles:
		particles_manager.stop_engine_flame(engine_particles)
		engine_particles = null

	var explosion_pos = rocket_image.position if rocket_image else Vector2.ZERO

	# Create explosion particles
	if particles_manager:
		particles_manager.create_explosion(explosion_pos)

	# Hide rocket
	if rocket_image:
		var tween = create_tween()
		tween.tween_property(rocket_image, "modulate:a", 0.0, 0.2)

	# Show explosion effect
	if explosion_label:
		explosion_label.text = "💥"
		explosion_label.position = explosion_pos
		explosion_label.visible = true
		explosion_label.modulate = Color.WHITE

		# Animate explosion
		var tween = create_tween()
		tween.set_ease(Tween.EASE_OUT)
		tween.set_trans(Tween.TRANS_ELASTIC)
		tween.tween_property(explosion_label, "scale", Vector2(3, 3), 0.3)
		tween.parallel().tween_property(explosion_label, "modulate:a", 0.0, EXPLOSION_DURATION)

		await tween.finished
		explosion_label.visible = false
		explosion_label.scale = Vector2.ONE

func show_success():
	is_launching = false

	# Stop engine flames
	if engine_particles:
		particles_manager.stop_engine_flame(engine_particles)
		engine_particles = null

	if rocket_image:
		# Rocket reaches orbit - celebrate!
		var tween = create_tween()
		tween.set_ease(Tween.EASE_OUT)
		tween.set_trans(Tween.TRANS_BACK)

		# Move to final position
		tween.tween_property(rocket_image, "position:y", -250, 0.5)

		# Add success glow
		tween.parallel().tween_property(rocket_image, "modulate", Color(0.5, 1.0, 0.5), 0.3)

		# Ensure rocket is horizontal for orbit (parallel to Earth's surface)
		tween.parallel().tween_property(rocket_image, "rotation", ORBIT_ROTATION, 0.5)

		await tween.finished

		# Create success sparkles
		if particles_manager:
			success_particles = particles_manager.create_success_sparkles(rocket_image.position)

		# Pulse effect
		var pulse_tween = create_tween()
		pulse_tween.set_loops(3)
		pulse_tween.tween_property(rocket_image, "scale", Vector2(1.2, 1.2), 0.3)
		pulse_tween.tween_property(rocket_image, "scale", Vector2.ONE, 0.3)
