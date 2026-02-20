extends Control

## Map content showing Earth and orbital destinations
## Draws a schematic diagram of reachable orbits with active flights and asset sidebar

var game_manager: GameManager = null

@onready var orbit_diagram = $HBoxContainer/CenterContainer/OrbitDiagram
@onready var sidebar_content = $HBoxContainer/SidebarPanel/SidebarScroll/SidebarContent

# Orbit colors
const LEO_COLOR = Color(0.3, 1.0, 0.3, 0.8)    # Green
const MEO_COLOR = Color(1.0, 1.0, 0.3, 0.8)    # Yellow
const GEO_COLOR = Color(1.0, 0.6, 0.2, 0.8)    # Orange
const LUNAR_COLOR = Color(0.8, 0.8, 1.0, 0.8)  # Light blue/white

# Flight colors
const FLIGHT_CONTRACT_COLOR = Color(0.3, 0.9, 1.0, 0.9)  # Cyan
const FLIGHT_DEPOT_COLOR = Color(0.9, 0.7, 0.3, 0.9)     # Amber

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

const DEPOT_COLOR = Color(0.9, 0.7, 0.3, 1.0)  # Amber/gold

# Position mapping for all 11 locations — radius and angle from center
# Locations on drawn orbit rings use exact ring radius; others are interpolated
var _location_positions: Dictionary = {
	"earth_surface": {"radius": 0.0, "angle": 0.0},
	"suborbital": {"radius": 70.0, "angle": PI * 0.8},
	"leo": {"radius": LEO_RADIUS, "angle": PI / 4},
	"sso": {"radius": (LEO_RADIUS + MEO_RADIUS) * 0.4, "angle": PI * 0.65},
	"meo": {"radius": MEO_RADIUS, "angle": PI / 3},
	"gto": {"radius": (MEO_RADIUS + GEO_RADIUS) * 0.5, "angle": PI * 0.45},
	"geo": {"radius": GEO_RADIUS, "angle": PI / 2.5},
	"l1": {"radius": (GEO_RADIUS + LUNAR_RADIUS) * 0.45, "angle": -PI * 0.15},
	"l2": {"radius": (GEO_RADIUS + LUNAR_RADIUS) * 0.55, "angle": -PI * 0.35},
	"lunar_orbit": {"radius": LUNAR_RADIUS, "angle": PI / 6},
	"lunar_surface": {"radius": LUNAR_RADIUS, "angle": -0.3},  # Same angle as moon
}

# Sidebar location display order
var _sidebar_location_order: Array = [
	"leo", "sso", "meo", "gto", "geo", "l1", "l2", "lunar_orbit", "lunar_surface"
]

func _ready():
	orbit_diagram.draw.connect(_on_orbit_diagram_draw)
	orbit_diagram.queue_redraw()

func set_game_manager(gm: GameManager):
	game_manager = gm

func refresh():
	orbit_diagram.queue_redraw()
	_update_sidebar()

func _on_date_changed(_new_day: int):
	if is_visible_in_tree():
		refresh()

func _on_flight_arrived(_flight_id: int, _destination: String, _reward: float):
	if is_visible_in_tree():
		refresh()

func _on_inventory_changed():
	if is_visible_in_tree():
		refresh()

func _on_visibility_changed():
	if is_visible_in_tree():
		refresh()

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

	if game_manager:
		# Draw transit paths (behind icons)
		_draw_transit_paths(center)

		# Draw deployed depots
		var depot_locations = game_manager.get_depot_locations()
		for loc in depot_locations:
			_draw_depot_indicator(center, loc)

		# Draw active flights (on top)
		_draw_active_flights(center)

	# Draw legend
	_draw_legend()

	# Also update sidebar when diagram redraws
	_update_sidebar()

# ==========================================
# Position helpers
# ==========================================

func _get_location_pos(center: Vector2, location_id: String) -> Vector2:
	var loc_data = _location_positions.get(location_id)
	if loc_data == null:
		return center
	return center + Vector2(loc_data["radius"], 0).rotated(loc_data["angle"])

func _get_location_short_name(location_id: String) -> String:
	match location_id:
		"earth_surface": return "EARTH"
		"suborbital": return "SUB"
		"leo": return "LEO"
		"sso": return "SSO"
		"meo": return "MEO"
		"gto": return "GTO"
		"geo": return "GEO"
		"l1": return "L1"
		"l2": return "L2"
		"lunar_orbit": return "LLO"
		"lunar_surface": return "MOON"
		_: return location_id.to_upper()

func _get_location_color(location_id: String) -> Color:
	match location_id:
		"earth_surface": return Color(0.6, 0.8, 1.0, 0.8)
		"suborbital": return LEO_COLOR
		"leo": return LEO_COLOR
		"sso": return LEO_COLOR
		"meo": return MEO_COLOR
		"gto": return GEO_COLOR
		"geo": return GEO_COLOR
		"l1": return LUNAR_COLOR
		"l2": return LUNAR_COLOR
		"lunar_orbit": return LUNAR_COLOR
		"lunar_surface": return LUNAR_COLOR
		_: return Color.WHITE

# ==========================================
# Flight drawing
# ==========================================

func _draw_transit_paths(center: Vector2):
	if not game_manager:
		return
	var count = game_manager.get_active_flight_count()
	for i in range(count):
		var days_remaining = game_manager.get_active_flight_days_remaining(i)
		if days_remaining <= 0:
			continue
		var current_loc = game_manager.get_active_flight_current_location(i)
		var next_dest = game_manager.get_active_flight_next_destination(i)
		if next_dest == "":
			continue
		var from_pos = _get_location_pos(center, current_loc)
		var to_pos = _get_location_pos(center, next_dest)
		# Dashed line with low alpha
		var payload_type = game_manager.get_active_flight_payload_type(i)
		var color = FLIGHT_CONTRACT_COLOR if payload_type == "contract" else FLIGHT_DEPOT_COLOR
		color.a = 0.3
		_draw_dashed_line(from_pos, to_pos, color, 1.0, 6.0)

func _draw_dashed_line(from: Vector2, to: Vector2, color: Color, width: float, dash_length: float):
	var dir = to - from
	var total_length = dir.length()
	if total_length < 1.0:
		return
	var normalized = dir / total_length
	var drawn = 0.0
	var drawing = true
	while drawn < total_length:
		var segment_end = min(drawn + dash_length, total_length)
		if drawing:
			var p1 = from + normalized * drawn
			var p2 = from + normalized * segment_end
			orbit_diagram.draw_line(p1, p2, color, width, true)
		drawn = segment_end
		drawing = !drawing

func _draw_active_flights(center: Vector2):
	if not game_manager:
		return
	var count = game_manager.get_active_flight_count()
	# Track how many flights are at each location for offset
	var location_flight_counts: Dictionary = {}

	for i in range(count):
		var current_loc = game_manager.get_active_flight_current_location(i)
		var days_remaining = game_manager.get_active_flight_days_remaining(i)
		var leg_transit_days = game_manager.get_active_flight_current_leg_transit_days(i)
		var next_dest = game_manager.get_active_flight_next_destination(i)
		var design_name = game_manager.get_active_flight_design_name(i)
		var payload_type = game_manager.get_active_flight_payload_type(i)
		var color = FLIGHT_CONTRACT_COLOR if payload_type == "contract" else FLIGHT_DEPOT_COLOR

		var pos: Vector2
		var label_text: String

		if days_remaining > 0 and leg_transit_days > 0 and next_dest != "":
			# In transit — interpolate between current location and next destination
			var leg_days_remaining = game_manager.get_active_flight_current_leg_days_remaining(i)
			var progress = 1.0 - (float(leg_days_remaining) / float(leg_transit_days))
			progress = clamp(progress, 0.0, 1.0)
			var from_pos = _get_location_pos(center, current_loc)
			var to_pos = _get_location_pos(center, next_dest)
			pos = from_pos.lerp(to_pos, progress)
			var dest_short = _get_location_short_name(game_manager.get_active_flight_destination(i))
			label_text = "%s → %s %dd" % [design_name, dest_short, days_remaining]
		else:
			# At location — offset from base position
			var loc_key = current_loc
			var offset_index = location_flight_counts.get(loc_key, 0) as int
			location_flight_counts[loc_key] = offset_index + 1
			pos = _get_location_pos(center, current_loc)
			# Offset each flight slightly to avoid overlap with depots
			pos += Vector2(0, -20 - offset_index * 14)
			label_text = design_name

		# Draw flight triangle icon
		_draw_flight_icon(pos, color)

		# Draw label
		var font = ThemeDB.fallback_font
		var label_pos = pos + Vector2(8, 4)
		orbit_diagram.draw_string(font, label_pos, label_text, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, color)

func _draw_flight_icon(pos: Vector2, color: Color):
	# Small triangle pointing right
	var size = 5.0
	var points = PackedVector2Array([
		pos + Vector2(-size, -size),
		pos + Vector2(size, 0),
		pos + Vector2(-size, size),
	])
	orbit_diagram.draw_colored_polygon(points, color)
	# Small outline
	orbit_diagram.draw_polyline(points, color.lightened(0.3), 1.0, true)

# ==========================================
# Sidebar
# ==========================================

func _update_sidebar():
	if sidebar_content == null or not game_manager:
		return

	# Clear existing sidebar children
	for child in sidebar_content.get_children():
		child.queue_free()

	# Title
	var title = Label.new()
	title.text = "Space Assets"
	title.add_theme_font_size_override("font_size", 14)
	title.add_theme_color_override("font_color", Color(0.9, 0.9, 0.9))
	sidebar_content.add_child(title)

	var sep = HSeparator.new()
	sidebar_content.add_child(sep)

	# Gather assets by location
	var assets_by_location: Dictionary = {}  # location_id -> Array of info dicts
	var in_transit_flights: Array = []

	# Depots
	var depot_locations = game_manager.get_depot_locations()
	for loc in depot_locations:
		if not assets_by_location.has(loc):
			assets_by_location[loc] = []
		var capacity = game_manager.get_depot_capacity(loc)
		var stored = game_manager.get_depot_total_stored(loc)
		assets_by_location[loc].append({
			"type": "depot",
			"text": "Depot: %s / %s kg" % [_format_mass(stored), _format_mass(capacity)],
		})

	# Active flights
	var flight_count = game_manager.get_active_flight_count()
	for i in range(flight_count):
		var current_loc = game_manager.get_active_flight_current_location(i)
		var days_remaining = game_manager.get_active_flight_days_remaining(i)
		var design_name = game_manager.get_active_flight_design_name(i)
		var destination = game_manager.get_active_flight_destination(i)
		var payload_type = game_manager.get_active_flight_payload_type(i)
		var dest_short = _get_location_short_name(destination)

		var flight_info = {
			"type": "flight",
			"payload_type": payload_type,
		}

		if days_remaining > 0:
			flight_info["text"] = "%s → %s (%dd)" % [design_name, dest_short, days_remaining]
			in_transit_flights.append(flight_info)
		else:
			flight_info["text"] = "%s (arrived)" % design_name
			if not assets_by_location.has(current_loc):
				assets_by_location[current_loc] = []
			assets_by_location[current_loc].append(flight_info)

	var has_any_assets = assets_by_location.size() > 0 or in_transit_flights.size() > 0

	if not has_any_assets:
		var placeholder = Label.new()
		placeholder.text = "No active assets"
		placeholder.add_theme_font_size_override("font_size", 12)
		placeholder.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		sidebar_content.add_child(placeholder)
		return

	# Show assets grouped by location
	for loc_id in _sidebar_location_order:
		if not assets_by_location.has(loc_id):
			continue

		var loc_color = _get_location_color(loc_id)
		var loc_name = _get_location_short_name(loc_id)

		# Location header
		var header = Label.new()
		header.text = loc_name
		header.add_theme_font_size_override("font_size", 12)
		header.add_theme_color_override("font_color", loc_color)
		sidebar_content.add_child(header)

		# Asset entries
		for asset in assets_by_location[loc_id]:
			var entry = Label.new()
			entry.text = "  " + asset["text"]
			entry.add_theme_font_size_override("font_size", 11)
			if asset["type"] == "depot":
				entry.add_theme_color_override("font_color", DEPOT_COLOR)
			elif asset.get("payload_type") == "contract":
				entry.add_theme_color_override("font_color", FLIGHT_CONTRACT_COLOR)
			else:
				entry.add_theme_color_override("font_color", FLIGHT_DEPOT_COLOR)
			sidebar_content.add_child(entry)

	# In-transit section
	if in_transit_flights.size() > 0:
		var spacer = Control.new()
		spacer.custom_minimum_size = Vector2(0, 4)
		sidebar_content.add_child(spacer)

		var transit_header = Label.new()
		transit_header.text = "In Transit"
		transit_header.add_theme_font_size_override("font_size", 12)
		transit_header.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		sidebar_content.add_child(transit_header)

		for flight in in_transit_flights:
			var entry = Label.new()
			entry.text = "  " + flight["text"]
			entry.add_theme_font_size_override("font_size", 11)
			var color = FLIGHT_CONTRACT_COLOR if flight["payload_type"] == "contract" else FLIGHT_DEPOT_COLOR
			entry.add_theme_color_override("font_color", color)
			sidebar_content.add_child(entry)

# ==========================================
# Existing drawing functions
# ==========================================

func _draw_orbit(center: Vector2, radius: float, color: Color, width: float):
	var segments = 64
	var _dash_length = 8
	for i in range(segments):
		if i % 2 == 0:
			var angle1 = (float(i) / segments) * TAU
			var angle2 = (float(i + 1) / segments) * TAU
			var p1 = center + Vector2(radius, 0).rotated(angle1)
			var p2 = center + Vector2(radius, 0).rotated(angle2)
			orbit_diagram.draw_line(p1, p2, color, width, true)

func _draw_earth(center: Vector2):
	orbit_diagram.draw_circle(center, EARTH_RADIUS + 8, EARTH_ATMOSPHERE)
	orbit_diagram.draw_circle(center, EARTH_RADIUS, EARTH_OCEAN)
	_draw_land_mass(center, EARTH_RADIUS * 0.3, 0.5, 0.8)
	_draw_land_mass(center, EARTH_RADIUS * 0.4, 2.0, 0.6)
	_draw_land_mass(center, EARTH_RADIUS * 0.25, 3.5, 0.5)
	_draw_land_mass(center, EARTH_RADIUS * 0.35, 5.0, 0.7)

func _draw_land_mass(center: Vector2, land_size: float, angle: float, scale_y: float):
	var land_center = center + Vector2(EARTH_RADIUS * 0.5, 0).rotated(angle)
	if land_center.distance_to(center) < EARTH_RADIUS - land_size * 0.3:
		var points = PackedVector2Array()
		for i in range(12):
			var a = (float(i) / 12) * TAU
			var offset = Vector2(cos(a) * land_size, sin(a) * land_size * scale_y)
			points.append(land_center + offset)
		orbit_diagram.draw_colored_polygon(points, EARTH_LAND)

func _draw_moon(pos: Vector2):
	orbit_diagram.draw_circle(pos, MOON_RADIUS, Color(0.7, 0.7, 0.7, 1.0))
	orbit_diagram.draw_circle(pos + Vector2(-4, -3), 3, Color(0.5, 0.5, 0.5, 1.0))
	orbit_diagram.draw_circle(pos + Vector2(3, 2), 2, Color(0.5, 0.5, 0.5, 1.0))

func _draw_depot_indicator(center: Vector2, location_id: String):
	var pos = _get_location_pos(center, location_id)
	if pos == center and location_id != "earth_surface":
		return

	var capacity = game_manager.get_depot_capacity(location_id)
	var stored = game_manager.get_depot_total_stored(location_id)
	var fill_ratio = stored / capacity if capacity > 0 else 0.0

	var fill_color: Color
	if fill_ratio < 0.5:
		fill_color = Color(0.3, 0.9, 0.3).lerp(Color(1.0, 1.0, 0.3), fill_ratio * 2.0)
	else:
		fill_color = Color(1.0, 1.0, 0.3).lerp(Color(1.0, 0.5, 0.2), (fill_ratio - 0.5) * 2.0)

	var icon_size = Vector2(12, 10)
	var icon_rect = Rect2(pos - icon_size / 2, icon_size)
	orbit_diagram.draw_rect(icon_rect, fill_color)
	orbit_diagram.draw_rect(icon_rect, DEPOT_COLOR, false, 1.5)

	var diamond_size = 4.0
	var diamond_top = pos + Vector2(0, -icon_size.y / 2 - diamond_size)
	var diamond_points = PackedVector2Array([
		diamond_top,
		pos + Vector2(diamond_size, -icon_size.y / 2),
		pos + Vector2(0, -icon_size.y / 2 + 1),
		pos + Vector2(-diamond_size, -icon_size.y / 2),
	])
	orbit_diagram.draw_colored_polygon(diamond_points, DEPOT_COLOR)

	var font = ThemeDB.fallback_font
	var loc_name = _get_location_display_name(location_id)
	var line1 = loc_name
	var line2 = "%s / %s kg" % [_format_mass(stored), _format_mass(capacity)]

	var text_x = pos.x + icon_size.x / 2 + 6
	orbit_diagram.draw_string(font, Vector2(text_x, pos.y - 2), line1, HORIZONTAL_ALIGNMENT_LEFT, -1, 11, DEPOT_COLOR)
	orbit_diagram.draw_string(font, Vector2(text_x, pos.y + 10), line2, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, fill_color)

func _get_location_display_name(location_id: String) -> String:
	match location_id:
		"leo": return "Low Earth Orbit"
		"sso": return "Sun-Synchronous Orbit"
		"meo": return "Medium Earth Orbit"
		"gto": return "Geostationary Transfer"
		"geo": return "Geostationary Orbit"
		"l1": return "Earth-Moon L1"
		"l2": return "Earth-Moon L2"
		"lunar_orbit": return "Lunar Orbit"
		"lunar_surface": return "Lunar Surface"
		_: return location_id

func _format_mass(kg: float) -> String:
	if kg >= 1000:
		return "%.1fK" % (kg / 1000.0)
	return "%.0f" % kg

func _draw_legend():
	var font = ThemeDB.fallback_font
	var x = 15.0
	var y = orbit_diagram.size.y - 20.0
	var has_legend = false

	# Flight legend entries
	if game_manager and game_manager.get_active_flight_count() > 0:
		# Contract flight icon
		_draw_flight_icon(Vector2(x + 5, y - 3), FLIGHT_CONTRACT_COLOR)
		orbit_diagram.draw_string(font, Vector2(x + 14, y + 2), "Contract Flight", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, FLIGHT_CONTRACT_COLOR)
		y -= 16.0
		# Depot flight icon
		_draw_flight_icon(Vector2(x + 5, y - 3), FLIGHT_DEPOT_COLOR)
		orbit_diagram.draw_string(font, Vector2(x + 14, y + 2), "Depot Delivery", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, FLIGHT_DEPOT_COLOR)
		y -= 16.0
		has_legend = true

	# Depot legend
	if game_manager and game_manager.get_depot_locations().size() > 0:
		var icon_rect = Rect2(x, y - 7, 10, 8)
		orbit_diagram.draw_rect(icon_rect, DEPOT_COLOR)
		orbit_diagram.draw_rect(icon_rect, DEPOT_COLOR, false, 1.0)
		orbit_diagram.draw_string(font, Vector2(x + 14, y + 2), "Deployed Depot", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, DEPOT_COLOR)

func _draw_orbit_label(center: Vector2, radius: float, text: String, color: Color, angle: float):
	var pos = center + Vector2(radius + 15, 0).rotated(angle)
	var font = ThemeDB.fallback_font
	var font_size = 12
	var text_size = font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
	var bg_rect = Rect2(pos - Vector2(2, text_size.y - 2), text_size + Vector2(4, 4))
	orbit_diagram.draw_rect(bg_rect, Color(0, 0, 0, 0.5))
	orbit_diagram.draw_string(font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, color)
