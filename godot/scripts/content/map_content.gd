extends Control

## Map content showing Earth and orbital destinations
## Draws a schematic diagram of reachable orbits

@onready var orbit_diagram = $CenterContainer/OrbitDiagram

# Orbit colors
const LEO_COLOR = Color(0.3, 1.0, 0.3, 0.8)    # Green
const MEO_COLOR = Color(1.0, 1.0, 0.3, 0.8)    # Yellow
const GEO_COLOR = Color(1.0, 0.6, 0.2, 0.8)    # Orange
const LUNAR_COLOR = Color(0.8, 0.8, 1.0, 0.8)  # Light blue/white

# Earth colors
const EARTH_OCEAN = Color(0.1, 0.3, 0.6, 1.0)
const EARTH_LAND = Color(0.2, 0.5, 0.2, 1.0)
const EARTH_ATMOSPHERE = Color(0.4, 0.6, 1.0, 0.3)

# Sizes (scaled for diagram)
const EARTH_RADIUS = 60.0
const LEO_RADIUS = 80.0     # ~400km scaled
const MEO_RADIUS = 150.0    # ~20,000km scaled
const GEO_RADIUS = 220.0    # ~36,000km scaled
const LUNAR_RADIUS = 350.0  # ~384,000km scaled (compressed)
const MOON_RADIUS = 16.0

func _ready():
	# Connect to redraw when the diagram control changes size
	orbit_diagram.draw.connect(_on_orbit_diagram_draw)
	orbit_diagram.queue_redraw()

func _on_orbit_diagram_draw():
	var center = orbit_diagram.size / 2

	# Draw orbits (back to front)
	_draw_orbit(center, LUNAR_RADIUS, LUNAR_COLOR, 2.0)
	_draw_orbit(center, GEO_RADIUS, GEO_COLOR, 2.0)
	_draw_orbit(center, MEO_RADIUS, MEO_COLOR, 2.0)
	_draw_orbit(center, LEO_RADIUS, LEO_COLOR, 2.0)

	# Draw Earth
	_draw_earth(center)

	# Draw Moon at lunar orbit distance
	var moon_pos = center + Vector2(LUNAR_RADIUS, 0).rotated(-0.3)
	_draw_moon(moon_pos)

	# Draw orbit labels
	_draw_orbit_label(center, LEO_RADIUS, "LEO", LEO_COLOR, -PI/4)
	_draw_orbit_label(center, MEO_RADIUS, "MEO", MEO_COLOR, -PI/3)
	_draw_orbit_label(center, GEO_RADIUS, "GEO", GEO_COLOR, -PI/2.5)
	_draw_orbit_label(center, LUNAR_RADIUS, "LUNAR", LUNAR_COLOR, -PI/6)

func _draw_orbit(center: Vector2, radius: float, color: Color, width: float):
	# Draw dashed orbit circle
	var segments = 64
	var dash_length = 8
	for i in range(segments):
		if i % 2 == 0:  # Create dashed effect
			var angle1 = (float(i) / segments) * TAU
			var angle2 = (float(i + 1) / segments) * TAU
			var p1 = center + Vector2(radius, 0).rotated(angle1)
			var p2 = center + Vector2(radius, 0).rotated(angle2)
			orbit_diagram.draw_line(p1, p2, color, width, true)

func _draw_earth(center: Vector2):
	# Draw atmosphere glow
	orbit_diagram.draw_circle(center, EARTH_RADIUS + 8, EARTH_ATMOSPHERE)

	# Draw ocean base
	orbit_diagram.draw_circle(center, EARTH_RADIUS, EARTH_OCEAN)

	# Draw simplified land masses
	# Just draw a few arcs/shapes to suggest continents
	var land_points = PackedVector2Array()

	# Simple continent shapes (very stylized)
	_draw_land_mass(center, EARTH_RADIUS * 0.3, 0.5, 0.8)
	_draw_land_mass(center, EARTH_RADIUS * 0.4, 2.0, 0.6)
	_draw_land_mass(center, EARTH_RADIUS * 0.25, 3.5, 0.5)
	_draw_land_mass(center, EARTH_RADIUS * 0.35, 5.0, 0.7)

func _draw_land_mass(center: Vector2, size: float, angle: float, scale_y: float):
	# Draw an ellipse-ish shape to represent a continent
	var land_center = center + Vector2(EARTH_RADIUS * 0.5, 0).rotated(angle)

	# Only draw if within Earth bounds
	if land_center.distance_to(center) < EARTH_RADIUS - size * 0.3:
		var points = PackedVector2Array()
		for i in range(12):
			var a = (float(i) / 12) * TAU
			var offset = Vector2(cos(a) * size, sin(a) * size * scale_y)
			points.append(land_center + offset)
		orbit_diagram.draw_colored_polygon(points, EARTH_LAND)

func _draw_moon(pos: Vector2):
	# Draw moon
	orbit_diagram.draw_circle(pos, MOON_RADIUS, Color(0.7, 0.7, 0.7, 1.0))
	# Add some crater-like darker spots
	orbit_diagram.draw_circle(pos + Vector2(-4, -3), 3, Color(0.5, 0.5, 0.5, 1.0))
	orbit_diagram.draw_circle(pos + Vector2(3, 2), 2, Color(0.5, 0.5, 0.5, 1.0))

func _draw_orbit_label(center: Vector2, radius: float, text: String, color: Color, angle: float):
	var pos = center + Vector2(radius + 15, 0).rotated(angle)
	var font = ThemeDB.fallback_font
	var font_size = 12

	# Draw text with small background
	var text_size = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
	var bg_rect = Rect2(pos - Vector2(2, text_size.y - 2), text_size + Vector2(4, 4))
	orbit_diagram.draw_rect(bg_rect, Color(0, 0, 0, 0.5))
	orbit_diagram.draw_string(font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, color)
