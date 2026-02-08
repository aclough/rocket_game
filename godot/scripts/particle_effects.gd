extends Node2D

# Particle effect manager for rocket launches

func create_engine_flame(spawn_position: Vector2) -> CPUParticles2D:
	var particles = CPUParticles2D.new()
	particles.position = spawn_position

	# Flame properties
	particles.emitting = true
	particles.amount = 20
	particles.lifetime = 0.5
	particles.one_shot = false
	particles.preprocess = 0.0
	particles.speed_scale = 1.5
	particles.explosiveness = 0.0
	particles.randomness = 0.5

	# Emission shape - cone downward
	particles.emission_shape = CPUParticles2D.EMISSION_SHAPE_SPHERE
	particles.emission_sphere_radius = 8.0

	# Direction and spread
	particles.direction = Vector2(0, 1)  # Downward
	particles.spread = 20.0
	particles.gravity = Vector2(0, 50)

	# Initial velocity
	particles.initial_velocity_min = 50.0
	particles.initial_velocity_max = 100.0

	# Scale
	particles.scale_amount_min = 2.0
	particles.scale_amount_max = 4.0
	particles.scale_amount_curve = create_scale_curve()

	# Color gradient (orange to yellow to transparent)
	var gradient = Gradient.new()
	gradient.add_point(0.0, Color(1.0, 0.5, 0.0, 1.0))  # Orange
	gradient.add_point(0.5, Color(1.0, 1.0, 0.0, 0.8))  # Yellow
	gradient.add_point(1.0, Color(1.0, 0.3, 0.0, 0.0))  # Fade out
	particles.color_ramp = gradient

	add_child(particles)
	return particles

func create_explosion(spawn_position: Vector2) -> CPUParticles2D:
	var particles = CPUParticles2D.new()
	particles.position = spawn_position

	# Explosion properties
	particles.emitting = true
	particles.amount = 50
	particles.lifetime = 1.0
	particles.one_shot = true
	particles.explosiveness = 1.0
	particles.randomness = 0.8

	# Emission shape - sphere
	particles.emission_shape = CPUParticles2D.EMISSION_SHAPE_SPHERE
	particles.emission_sphere_radius = 10.0

	# Direction - all directions
	particles.direction = Vector2(0, -1)
	particles.spread = 180.0
	particles.gravity = Vector2(0, 200)

	# Initial velocity
	particles.initial_velocity_min = 100.0
	particles.initial_velocity_max = 300.0

	# Scale
	particles.scale_amount_min = 3.0
	particles.scale_amount_max = 6.0
	particles.scale_amount_curve = create_explosion_scale_curve()

	# Color gradient (bright orange/red to dark)
	var gradient = Gradient.new()
	gradient.add_point(0.0, Color(1.0, 1.0, 0.5, 1.0))  # Bright yellow
	gradient.add_point(0.3, Color(1.0, 0.3, 0.0, 1.0))  # Orange
	gradient.add_point(0.6, Color(0.5, 0.0, 0.0, 0.8))  # Dark red
	gradient.add_point(1.0, Color(0.2, 0.2, 0.2, 0.0))  # Smoke fade
	particles.color_ramp = gradient

	add_child(particles)

	# Auto-cleanup after animation
	await get_tree().create_timer(2.0).timeout
	particles.queue_free()

	return particles

func create_success_sparkles(spawn_position: Vector2) -> CPUParticles2D:
	var particles = CPUParticles2D.new()
	particles.position = spawn_position

	# Sparkle properties
	particles.emitting = true
	particles.amount = 30
	particles.lifetime = 1.5
	particles.one_shot = false
	particles.explosiveness = 0.2
	particles.randomness = 0.7

	# Emission shape - circle around rocket
	particles.emission_shape = CPUParticles2D.EMISSION_SHAPE_SPHERE
	particles.emission_sphere_radius = 30.0

	# Direction - upward and outward
	particles.direction = Vector2(0, -1)
	particles.spread = 60.0
	particles.gravity = Vector2(0, -20)  # Float upward

	# Initial velocity
	particles.initial_velocity_min = 20.0
	particles.initial_velocity_max = 60.0

	# Scale
	particles.scale_amount_min = 1.5
	particles.scale_amount_max = 3.0
	particles.scale_amount_curve = create_sparkle_scale_curve()

	# Color gradient (bright colors)
	var gradient = Gradient.new()
	gradient.add_point(0.0, Color(1.0, 1.0, 1.0, 1.0))  # White
	gradient.add_point(0.3, Color(0.5, 1.0, 0.5, 1.0))  # Green
	gradient.add_point(0.6, Color(0.3, 1.0, 1.0, 0.8))  # Cyan
	gradient.add_point(1.0, Color(0.5, 1.0, 0.5, 0.0))  # Fade to green
	particles.color_ramp = gradient

	add_child(particles)
	return particles

func create_scale_curve() -> Curve:
	var curve = Curve.new()
	curve.add_point(Vector2(0.0, 1.0))
	curve.add_point(Vector2(0.5, 1.2))
	curve.add_point(Vector2(1.0, 0.0))
	return curve

func create_explosion_scale_curve() -> Curve:
	var curve = Curve.new()
	curve.add_point(Vector2(0.0, 0.5))
	curve.add_point(Vector2(0.2, 1.5))
	curve.add_point(Vector2(0.5, 1.0))
	curve.add_point(Vector2(1.0, 0.2))
	return curve

func create_sparkle_scale_curve() -> Curve:
	var curve = Curve.new()
	curve.add_point(Vector2(0.0, 0.0))
	curve.add_point(Vector2(0.3, 1.0))
	curve.add_point(Vector2(0.7, 0.8))
	curve.add_point(Vector2(1.0, 0.0))
	return curve

func stop_engine_flame(particles: CPUParticles2D):
	if particles and is_instance_valid(particles):
		particles.emitting = false
		# Clean up after particles finish
		await get_tree().create_timer(1.0).timeout
		if is_instance_valid(particles):
			particles.queue_free()

func stop_success_sparkles(particles: CPUParticles2D):
	if particles and is_instance_valid(particles):
		particles.emitting = false
		# Clean up after particles finish
		await get_tree().create_timer(2.0).timeout
		if is_instance_valid(particles):
			particles.queue_free()
