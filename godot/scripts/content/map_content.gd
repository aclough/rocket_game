extends Control

## Map content showing Earth and orbital destinations
## Draws a schematic diagram of reachable orbits

var game_manager: GameManager = null

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

const DEPOT_COLOR = Color(0.9, 0.7, 0.3, 1.0)  # Amber/gold

# Map location IDs to orbit radii
var _orbit_radii: Dictionary = {
	"leo": LEO_RADIUS,
	"meo": MEO_RADIUS,
	"geo": GEO_RADIUS,
	"lunar_orbit": LUNAR_RADIUS,
}

# Map location IDs to orbit angles for depot placement
var _orbit_angles: Dictionary = {
	"leo": PI/4,
	"meo": PI/3,
	"geo": PI/2.5,
	"lunar_orbit": PI/6,
}

func set_game_manager(gm: GameManager):
	game_manager = gm

func refresh():
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

	# Draw deployed depots
	if game_manager:
		var depot_locations = game_manager.get_depot_locations()
		for loc in depot_locations:
			_draw_depot_indicator(center, loc)

	# Draw legend
	_draw_legend()

func _draw_orbit(center: Vector2, radius: float, color: Color, width: float):
	# Draw dashed orbit circle
	var segments = 64
	var _dash_length = 8
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
	var _land_points = PackedVector2Array()

	# Simple continent shapes (very stylized)
	_draw_land_mass(center, EARTH_RADIUS * 0.3, 0.5, 0.8)
	_draw_land_mass(center, EARTH_RADIUS * 0.4, 2.0, 0.6)
	_draw_land_mass(center, EARTH_RADIUS * 0.25, 3.5, 0.5)
	_draw_land_mass(center, EARTH_RADIUS * 0.35, 5.0, 0.7)

func _draw_land_mass(center: Vector2, land_size: float, angle: float, scale_y: float):
	# Draw an ellipse-ish shape to represent a continent
	var land_center = center + Vector2(EARTH_RADIUS * 0.5, 0).rotated(angle)

	# Only draw if within Earth bounds
	if land_center.distance_to(center) < EARTH_RADIUS - land_size * 0.3:
		var points = PackedVector2Array()
		for i in range(12):
			var a = (float(i) / 12) * TAU
			var offset = Vector2(cos(a) * land_size, sin(a) * land_size * scale_y)
			points.append(land_center + offset)
		orbit_diagram.draw_colored_polygon(points, EARTH_LAND)

func _draw_moon(pos: Vector2):
	# Draw moon
	orbit_diagram.draw_circle(pos, MOON_RADIUS, Color(0.7, 0.7, 0.7, 1.0))
	# Add some crater-like darker spots
	orbit_diagram.draw_circle(pos + Vector2(-4, -3), 3, Color(0.5, 0.5, 0.5, 1.0))
	orbit_diagram.draw_circle(pos + Vector2(3, 2), 2, Color(0.5, 0.5, 0.5, 1.0))

func _draw_depot_indicator(center: Vector2, location_id: String):
	var radius = _orbit_radii.get(location_id, 0.0) as float
	var angle = _orbit_angles.get(location_id, 0.0) as float
	if radius == 0:
		return

	var pos = center + Vector2(radius, 0).rotated(angle)

	var capacity = game_manager.get_depot_capacity(location_id)
	var stored = game_manager.get_depot_total_stored(location_id)
	var fill_ratio = stored / capacity if capacity > 0 else 0.0

	# Color-code by fill level: green (empty) -> yellow (half) -> orange (full)
	var fill_color: Color
	if fill_ratio < 0.5:
		fill_color = Color(0.3, 0.9, 0.3).lerp(Color(1.0, 1.0, 0.3), fill_ratio * 2.0)
	else:
		fill_color = Color(1.0, 1.0, 0.3).lerp(Color(1.0, 0.5, 0.2), (fill_ratio - 0.5) * 2.0)

	# Draw depot icon — larger rectangle with border
	var icon_size = Vector2(12, 10)
	var icon_rect = Rect2(pos - icon_size / 2, icon_size)
	orbit_diagram.draw_rect(icon_rect, fill_color)
	orbit_diagram.draw_rect(icon_rect, DEPOT_COLOR, false, 1.5)

	# Draw a small diamond on top of the rectangle
	var diamond_size = 4.0
	var diamond_top = pos + Vector2(0, -icon_size.y / 2 - diamond_size)
	var diamond_points = PackedVector2Array([
		diamond_top,
		pos + Vector2(diamond_size, -icon_size.y / 2),
		pos + Vector2(0, -icon_size.y / 2 + 1),
		pos + Vector2(-diamond_size, -icon_size.y / 2),
	])
	orbit_diagram.draw_colored_polygon(diamond_points, DEPOT_COLOR)

	# Info text: location name + fuel stored / capacity
	var font = ThemeDB.fallback_font
	var loc_name = _get_location_display_name(location_id)
	var info_text = "%s\n%s / %s kg" % [loc_name, _format_mass(stored), _format_mass(capacity)]
	var line1 = loc_name
	var line2 = "%s / %s kg" % [_format_mass(stored), _format_mass(capacity)]

	var text_x = pos.x + icon_size.x / 2 + 6
	orbit_diagram.draw_string(font, Vector2(text_x, pos.y - 2), line1, HORIZONTAL_ALIGNMENT_LEFT, -1, 11, DEPOT_COLOR)
	orbit_diagram.draw_string(font, Vector2(text_x, pos.y + 10), line2, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, fill_color)

func _get_location_display_name(location_id: String) -> String:
	match location_id:
		"leo": return "Low Earth Orbit"
		"meo": return "Medium Earth Orbit"
		"geo": return "Geostationary Orbit"
		"lunar_orbit": return "Lunar Orbit"
		_: return location_id

func _format_mass(kg: float) -> String:
	if kg >= 1000:
		return "%.1fK" % (kg / 1000.0)
	return "%.0f" % kg

func _draw_legend():
	var font = ThemeDB.fallback_font
	var x = 15.0
	var y = orbit_diagram.size.y - 20.0

	# Only show depot legend if there are deployed depots
	if game_manager and game_manager.get_depot_locations().size() > 0:
		# Draw small depot icon
		var icon_rect = Rect2(x, y - 7, 10, 8)
		orbit_diagram.draw_rect(icon_rect, DEPOT_COLOR)
		orbit_diagram.draw_rect(icon_rect, DEPOT_COLOR, false, 1.0)
		orbit_diagram.draw_string(font, Vector2(x + 14, y + 2), "Deployed Depot", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, DEPOT_COLOR)

func _draw_orbit_label(center: Vector2, radius: float, text: String, color: Color, angle: float):
	var pos = center + Vector2(radius + 15, 0).rotated(angle)
	var font = ThemeDB.fallback_font
	var font_size = 12

	# Draw text with small background
	var text_size = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
	var bg_rect = Rect2(pos - Vector2(2, text_size.y - 2), text_size + Vector2(4, 4))
	orbit_diagram.draw_rect(bg_rect, Color(0, 0, 0, 0.5))
	orbit_diagram.draw_string(font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, color)
