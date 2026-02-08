extends Control

## Main game shell with tabbed interface
## Handles status bar updates and tab switching

enum Tab { MAP, MISSIONS, DESIGN, LAUNCH_SITE, RESEARCH, PRODUCTION, FINANCE }

var current_tab: Tab = Tab.MAP

# GameManager reference
@onready var game_manager: GameManager = $GameManager

# Time pause state for UI
var _last_paused_state: bool = false

# Status bar labels
@onready var fame_label = $MainVBox/StatusBar/StatusMargin/StatusHBox/FameContainer/FameLabel
@onready var money_label = $MainVBox/StatusBar/StatusMargin/StatusHBox/MoneyLabel
@onready var date_label = $MainVBox/StatusBar/StatusMargin/StatusHBox/DateLabel
@onready var pause_indicator = $MainVBox/StatusBar/StatusMargin/StatusHBox/PauseIndicator

# Content areas
@onready var content_areas: Dictionary = {
	Tab.MAP: $MainVBox/ContentHBox/ContentArea/MapContent,
	Tab.MISSIONS: $MainVBox/ContentHBox/ContentArea/MissionsContent,
	Tab.DESIGN: $MainVBox/ContentHBox/ContentArea/DesignContent,
	Tab.LAUNCH_SITE: $MainVBox/ContentHBox/ContentArea/LaunchSiteContent,
	Tab.RESEARCH: $MainVBox/ContentHBox/ContentArea/ResearchContent,
	Tab.FINANCE: $MainVBox/ContentHBox/ContentArea/FinanceContent,
	Tab.PRODUCTION: $MainVBox/ContentHBox/ContentArea/ProductionContent,
}

# Tab buttons
@onready var tab_buttons: Dictionary = {
	Tab.MAP: $MainVBox/TabBar/TabMargin/TabHBox/MapTab,
	Tab.MISSIONS: $MainVBox/TabBar/TabMargin/TabHBox/MissionsTab,
	Tab.DESIGN: $MainVBox/TabBar/TabMargin/TabHBox/DesignTab,
	Tab.LAUNCH_SITE: $MainVBox/TabBar/TabMargin/TabHBox/LaunchSiteTab,
	Tab.RESEARCH: $MainVBox/TabBar/TabMargin/TabHBox/ResearchTab,
	Tab.FINANCE: $MainVBox/TabBar/TabMargin/TabHBox/FinanceTab,
	Tab.PRODUCTION: $MainVBox/TabBar/TabMargin/TabHBox/ProductionTab,
}

# Launch overlay
@onready var launch_overlay = $LaunchOverlay

# Research tab UI elements (built dynamically)
var _research_teams_container: VBoxContainer
var _research_team_count_label: Label
var _research_salary_label: Label
var _research_designs_container: VBoxContainer
var _research_no_designs_label: Label
var _research_engines_container: VBoxContainer
var _research_update_timer: Timer
var _expanded_design_cards: Dictionary = {}  # design_index -> bool
var _expanded_engine_cards: Dictionary = {}  # engine_index -> bool

# Production tab UI elements (built dynamically)
var _prod_mfg_team_count_label: Label
var _prod_mfg_salary_label: Label
var _prod_mfg_teams_container: VBoxContainer
var _prod_floor_space_label: Label
var _prod_orders_container: VBoxContainer
var _prod_no_orders_label: Label
var _prod_engine_inv_container: VBoxContainer
var _prod_rocket_inv_container: VBoxContainer
var _prod_update_timer: Timer

# Finance tab UI elements (built dynamically)
var _finance_balance_label: Label
var _finance_burn_label: Label
var _finance_salary_date_label: Label
var _finance_runway_label: Label
var _finance_eng_header_label: Label
var _finance_mfg_header_label: Label
var _finance_eng_teams_container: VBoxContainer
var _finance_mfg_teams_container: VBoxContainer
var _finance_prices_container: VBoxContainer

# Toast notification stacking
var _active_toasts: Array = []

func _ready():
	# Connect GameManager signals
	game_manager.money_changed.connect(_on_money_changed)
	game_manager.date_changed.connect(_on_date_changed)
	game_manager.fame_changed.connect(_on_fame_changed)
	game_manager.time_paused.connect(_on_time_paused)
	game_manager.time_resumed.connect(_on_time_resumed)
	game_manager.work_event_occurred.connect(_on_work_event)

	# Connect manufacturing signals
	game_manager.manufacturing_changed.connect(_on_manufacturing_changed)
	game_manager.inventory_changed.connect(_on_inventory_changed)

	# Initialize content areas with game manager
	_setup_missions_content()
	_setup_design_content()
	_setup_launch_site_content()
	_setup_launch_overlay()
	_setup_research_content()
	_setup_production_content()
	_setup_finance_content()

	# Initial status bar update
	_update_status_bar()
	_update_pause_indicator()

	# Create timer for periodic research UI updates
	_research_update_timer = Timer.new()
	_research_update_timer.wait_time = 0.5
	_research_update_timer.timeout.connect(_on_research_update_timer)
	add_child(_research_update_timer)
	_research_update_timer.start()

	# Show default tab (Map)
	_show_tab(Tab.MAP)

func _process(delta: float):
	# Advance game time (handles pausing internally)
	var _events = game_manager.advance_time(delta)
	# Events will be handled via signals

func _input(event: InputEvent):
	# Spacebar toggles pause
	if event.is_action_pressed("ui_accept"):  # Space is typically mapped to ui_accept
		game_manager.toggle_time_pause()
		get_viewport().set_input_as_handled()

func _setup_missions_content():
	var missions = content_areas[Tab.MISSIONS]
	missions.set_game_manager(game_manager)
	missions.contract_selected.connect(_on_contract_selected)
	missions.design_requested.connect(_on_design_requested)

func _setup_design_content():
	var design = content_areas[Tab.DESIGN]
	design.set_game_manager(game_manager)
	design.testing_requested.connect(_on_testing_requested)
	design.back_requested.connect(_on_design_back_requested)

func _setup_launch_site_content():
	var launch_site = content_areas[Tab.LAUNCH_SITE]
	launch_site.set_game_manager(game_manager)
	launch_site.launch_requested.connect(_on_launch_requested)

func _setup_launch_overlay():
	launch_overlay.launch_completed.connect(_on_launch_completed)

func _setup_research_content():
	var research = content_areas[Tab.RESEARCH]
	if not research:
		return

	# Remove placeholder label
	for child in research.get_children():
		child.queue_free()

	# Build the Research UI
	var margin = MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", 20)
	margin.add_theme_constant_override("margin_top", 20)
	margin.add_theme_constant_override("margin_right", 20)
	margin.add_theme_constant_override("margin_bottom", 20)
	research.add_child(margin)

	var main_hbox = HBoxContainer.new()
	main_hbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main_hbox.size_flags_vertical = Control.SIZE_EXPAND_FILL
	main_hbox.add_theme_constant_override("separation", 20)
	margin.add_child(main_hbox)

	# Teams Panel (left side)
	var teams_panel = PanelContainer.new()
	teams_panel.custom_minimum_size = Vector2(300, 0)
	teams_panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	main_hbox.add_child(teams_panel)

	var teams_margin = MarginContainer.new()
	teams_margin.add_theme_constant_override("margin_left", 15)
	teams_margin.add_theme_constant_override("margin_top", 15)
	teams_margin.add_theme_constant_override("margin_right", 15)
	teams_margin.add_theme_constant_override("margin_bottom", 15)
	teams_panel.add_child(teams_margin)

	var teams_vbox = VBoxContainer.new()
	teams_vbox.size_flags_vertical = Control.SIZE_EXPAND_FILL
	teams_vbox.add_theme_constant_override("separation", 12)
	teams_margin.add_child(teams_vbox)

	var teams_title = Label.new()
	teams_title.text = "Engineering Teams"
	teams_title.add_theme_font_size_override("font_size", 20)
	teams_vbox.add_child(teams_title)

	var header_hbox = HBoxContainer.new()
	header_hbox.add_theme_constant_override("separation", 10)
	teams_vbox.add_child(header_hbox)

	_research_team_count_label = Label.new()
	_research_team_count_label.text = "Teams: 0"
	_research_team_count_label.add_theme_font_size_override("font_size", 14)
	header_hbox.add_child(_research_team_count_label)

	var spacer = Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header_hbox.add_child(spacer)

	var hire_btn = Button.new()
	hire_btn.text = "+ Hire Eng Team ($150K)"
	hire_btn.add_theme_font_size_override("font_size", 14)
	hire_btn.pressed.connect(_on_research_hire_pressed)
	header_hbox.add_child(hire_btn)

	_research_salary_label = Label.new()
	_research_salary_label.text = "Monthly salary: $0"
	_research_salary_label.add_theme_font_size_override("font_size", 12)
	_research_salary_label.add_theme_color_override("font_color", Color(1, 0.85, 0.3))
	teams_vbox.add_child(_research_salary_label)

	var drag_hint = Label.new()
	drag_hint.text = "Drag teams to work items on the right"
	drag_hint.add_theme_font_size_override("font_size", 11)
	drag_hint.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	teams_vbox.add_child(drag_hint)

	var teams_scroll = ScrollContainer.new()
	teams_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	teams_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	teams_vbox.add_child(teams_scroll)

	_research_teams_container = VBoxContainer.new()
	_research_teams_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_research_teams_container.add_theme_constant_override("separation", 8)
	teams_scroll.add_child(_research_teams_container)

	# Work Panel (right side)
	var work_panel = PanelContainer.new()
	work_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	work_panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	main_hbox.add_child(work_panel)

	var work_margin = MarginContainer.new()
	work_margin.add_theme_constant_override("margin_left", 15)
	work_margin.add_theme_constant_override("margin_top", 15)
	work_margin.add_theme_constant_override("margin_right", 15)
	work_margin.add_theme_constant_override("margin_bottom", 15)
	work_panel.add_child(work_margin)

	var work_vbox = VBoxContainer.new()
	work_vbox.size_flags_vertical = Control.SIZE_EXPAND_FILL
	work_vbox.add_theme_constant_override("separation", 15)
	work_margin.add_child(work_vbox)

	var work_title = Label.new()
	work_title.text = "Work In Progress"
	work_title.add_theme_font_size_override("font_size", 20)
	work_vbox.add_child(work_title)

	var designs_section = VBoxContainer.new()
	designs_section.size_flags_vertical = Control.SIZE_EXPAND_FILL
	designs_section.add_theme_constant_override("separation", 10)
	work_vbox.add_child(designs_section)

	var designs_label = Label.new()
	designs_label.text = "Rocket Designs"
	designs_label.add_theme_font_size_override("font_size", 16)
	designs_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
	designs_section.add_child(designs_label)

	_research_no_designs_label = Label.new()
	_research_no_designs_label.text = "No designs in progress. Submit a design from the Design tab to begin engineering."
	_research_no_designs_label.add_theme_font_size_override("font_size", 14)
	_research_no_designs_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	_research_no_designs_label.autowrap_mode = TextServer.AUTOWRAP_WORD
	designs_section.add_child(_research_no_designs_label)

	var designs_scroll = ScrollContainer.new()
	designs_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	designs_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	designs_section.add_child(designs_scroll)

	_research_designs_container = VBoxContainer.new()
	_research_designs_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_research_designs_container.add_theme_constant_override("separation", 10)
	designs_scroll.add_child(_research_designs_container)

	# Engines section
	var engines_section = VBoxContainer.new()
	engines_section.size_flags_vertical = Control.SIZE_EXPAND_FILL
	engines_section.add_theme_constant_override("separation", 10)
	work_vbox.add_child(engines_section)

	var engines_label = Label.new()
	engines_label.text = "Engines"
	engines_label.add_theme_font_size_override("font_size", 16)
	engines_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
	engines_section.add_child(engines_label)

	var engines_scroll = ScrollContainer.new()
	engines_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	engines_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	engines_section.add_child(engines_scroll)

	_research_engines_container = VBoxContainer.new()
	_research_engines_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_research_engines_container.add_theme_constant_override("separation", 10)
	engines_scroll.add_child(_research_engines_container)

	# Connect signals
	game_manager.teams_changed.connect(_on_research_teams_changed)
	game_manager.designs_changed.connect(_on_research_designs_changed)

	# Initial update
	_update_research_ui()

func _update_research_ui():
	_update_research_teams()
	_update_research_designs()
	_update_research_engines()

func _update_research_teams():
	if not _research_teams_container:
		return

	# Update labels (engineering teams only)
	var eng_ids = game_manager.get_engineering_team_ids()
	var count = eng_ids.size()
	if _research_team_count_label:
		_research_team_count_label.text = "Teams: %d" % count
	if _research_salary_label:
		var salary = game_manager.get_engineering_monthly_salary()
		_research_salary_label.text = "Monthly salary: $%.0fK" % (salary / 1000.0)

	# Clear and rebuild team cards (engineering teams only)
	for child in _research_teams_container.get_children():
		child.queue_free()

	for id in eng_ids:
		var card = _create_team_card(id)
		_research_teams_container.add_child(card)

func _create_team_card(team_id: int) -> PanelContainer:
	var panel = PanelContainer.new()
	panel.custom_minimum_size = Vector2(0, 70)

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 10)
	margin.add_theme_constant_override("margin_top", 8)
	margin.add_theme_constant_override("margin_right", 10)
	margin.add_theme_constant_override("margin_bottom", 8)
	panel.add_child(margin)

	var hbox = HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 8)
	margin.add_child(hbox)

	var vbox = VBoxContainer.new()
	vbox.add_theme_constant_override("separation", 4)
	vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hbox.add_child(vbox)

	var name_label = Label.new()
	name_label.text = game_manager.get_team_name(team_id)
	name_label.add_theme_font_size_override("font_size", 14)
	vbox.add_child(name_label)

	var status_label = Label.new()
	var is_ramping = game_manager.is_team_ramping_up(team_id)
	var is_assigned = game_manager.is_team_assigned(team_id)

	if is_ramping:
		var days = game_manager.get_team_ramp_up_days(team_id)
		status_label.text = "Ramping up (%d days)" % days
		status_label.add_theme_color_override("font_color", Color(1.0, 0.6, 0.2))
		panel.modulate = Color(1.0, 0.9, 0.7)
	elif is_assigned:
		var assignment = game_manager.get_team_assignment(team_id)
		var atype = assignment.get("type", "none")
		if atype == "design":
			var design_index = assignment.get("design_index", -1)
			if design_index >= 0:
				var design_name = game_manager.get_rocket_design_name(design_index)
				status_label.text = "Working on: %s" % design_name
			else:
				status_label.text = "Working on design"
		elif atype == "engine":
			var engine_id = assignment.get("engine_design_id", -1)
			if engine_id >= 0:
				var engine_name = game_manager.get_engine_type_name(engine_id)
				status_label.text = "Refining: %s" % engine_name
			else:
				status_label.text = "Refining engine"
		elif atype == "manufacturing":
			var order_id = assignment.get("order_id", -1)
			if order_id >= 0:
				var order_info = game_manager.get_order_info(order_id)
				var order_name = order_info.get("display_name", "Unknown")
				status_label.text = "Building: %s" % order_name
			else:
				status_label.text = "Manufacturing"
			status_label.add_theme_color_override("font_color", Color(0.8, 0.6, 1.0))
		else:
			status_label.text = "Assigned"
		if atype != "manufacturing":
			status_label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
	else:
		status_label.text = "Available"
		status_label.add_theme_color_override("font_color", Color(0.5, 1.0, 0.5))

	status_label.add_theme_font_size_override("font_size", 11)
	vbox.add_child(status_label)

	# Add unassign button if team is assigned
	if is_assigned:
		var unassign_btn = Button.new()
		unassign_btn.text = "X"
		unassign_btn.tooltip_text = "Unassign team"
		unassign_btn.custom_minimum_size = Vector2(30, 30)
		unassign_btn.add_theme_font_size_override("font_size", 12)
		unassign_btn.pressed.connect(_on_team_unassign_pressed.bind(team_id))
		hbox.add_child(unassign_btn)

	return panel

func _on_team_unassign_pressed(team_id: int):
	game_manager.unassign_team(team_id)
	_update_research_ui()

func _update_research_designs():
	if not _research_designs_container:
		return

	# Clear existing
	for child in _research_designs_container.get_children():
		child.queue_free()

	var design_count = game_manager.get_rocket_design_count()
	var has_work_items = false

	for i in range(design_count):
		var base_status = game_manager.get_design_status_base(i)
		if base_status == "Engineering" or base_status == "Refining" or base_status == "Fixing":
			var card = _create_design_work_card(i)
			_research_designs_container.add_child(card)
			has_work_items = true

	if _research_no_designs_label:
		_research_no_designs_label.visible = not has_work_items

func _create_design_work_card(index: int) -> PanelContainer:
	var design_name = game_manager.get_rocket_design_name(index)
	var status = game_manager.get_design_status(index)
	var base_status = game_manager.get_design_status_base(index)
	var progress = game_manager.get_design_progress(index)
	var teams_count = game_manager.get_teams_on_design_count(index)
	var is_expanded = _expanded_design_cards.get(index, false)

	var panel = PanelContainer.new()
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	# Style based on status
	var style = StyleBoxFlat.new()
	if base_status == "Refining":
		# Blue style for Refining
		style.set_bg_color(Color(0.08, 0.1, 0.18))
		style.set_border_color(Color(0.3, 0.5, 0.9, 0.5))
	elif base_status == "Fixing":
		# Orange style for Fixing
		style.set_bg_color(Color(0.18, 0.12, 0.08))
		style.set_border_color(Color(0.9, 0.6, 0.3, 0.5))
	else:
		# Default blue for Engineering
		style.set_bg_color(Color(0.08, 0.1, 0.15))
		style.set_border_color(Color(0.3, 0.5, 0.8, 0.5))
	style.set_border_width_all(2)
	style.set_corner_radius_all(4)
	panel.add_theme_stylebox_override("panel", style)

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 15)
	margin.add_theme_constant_override("margin_right", 15)
	margin.add_theme_constant_override("margin_top", 12)
	margin.add_theme_constant_override("margin_bottom", 12)
	panel.add_child(margin)

	var vbox = VBoxContainer.new()
	vbox.add_theme_constant_override("separation", 6)
	margin.add_child(vbox)

	var header = HBoxContainer.new()
	vbox.add_child(header)

	# Make design name a clickable button to expand/collapse
	var name_btn = Button.new()
	name_btn.text = ("▼ " if is_expanded else "▶ ") + design_name
	name_btn.add_theme_font_size_override("font_size", 18)
	name_btn.flat = true
	name_btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
	name_btn.pressed.connect(_on_design_card_toggle.bind(index))
	header.add_child(name_btn)

	var status_label = Label.new()
	status_label.text = status
	status_label.add_theme_font_size_override("font_size", 14)
	if base_status == "Refining":
		status_label.add_theme_color_override("font_color", Color(0.4, 0.6, 1.0))
	elif base_status == "Fixing":
		status_label.add_theme_color_override("font_color", Color(1.0, 0.7, 0.3))
	else:
		status_label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
	header.add_child(status_label)

	if teams_count > 0:
		var teams_on = _get_teams_on_design(index)
		for tid in teams_on:
			var tname_label = Label.new()
			tname_label.text = game_manager.get_team_name(tid)
			tname_label.add_theme_font_size_override("font_size", 12)
			tname_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
			vbox.add_child(tname_label)
	else:
		var teams_label = Label.new()
		teams_label.text = "No teams assigned - drag a team here"
		teams_label.add_theme_color_override("font_color", Color(0.8, 0.6, 0.3))
		teams_label.add_theme_font_size_override("font_size", 12)
		vbox.add_child(teams_label)

	# Progress bar - different styles for different phases
	if base_status == "Refining":
		# Refining: blue bar always at 100%
		var progress_bar = ProgressBar.new()
		progress_bar.value = 100
		progress_bar.custom_minimum_size = Vector2(0, 12)
		progress_bar.show_percentage = false
		# Style the progress bar blue
		var fill_style = StyleBoxFlat.new()
		fill_style.set_bg_color(Color(0.3, 0.5, 0.9))
		fill_style.set_corner_radius_all(3)
		progress_bar.add_theme_stylebox_override("fill", fill_style)
		vbox.add_child(progress_bar)
	elif base_status == "Fixing":
		# Fixing: normal progress bar (orange tinted)
		var progress_bar = ProgressBar.new()
		progress_bar.value = progress * 100
		progress_bar.custom_minimum_size = Vector2(0, 12)
		progress_bar.show_percentage = true
		var fill_style = StyleBoxFlat.new()
		fill_style.set_bg_color(Color(0.9, 0.6, 0.3))
		fill_style.set_corner_radius_all(3)
		progress_bar.add_theme_stylebox_override("fill", fill_style)
		vbox.add_child(progress_bar)
	elif base_status == "Engineering" and progress > 0:
		# Engineering: normal progress bar
		var progress_bar = ProgressBar.new()
		progress_bar.value = progress * 100
		progress_bar.custom_minimum_size = Vector2(0, 12)
		progress_bar.show_percentage = true
		vbox.add_child(progress_bar)
	elif base_status == "Engineering":
		var hint_label = Label.new()
		hint_label.text = "Waiting for team assignment to begin work"
		hint_label.add_theme_font_size_override("font_size", 11)
		hint_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		vbox.add_child(hint_label)

	# Expanded section: show flaws
	if is_expanded:
		var separator = HSeparator.new()
		vbox.add_child(separator)

		var flaws_section = VBoxContainer.new()
		flaws_section.add_theme_constant_override("separation", 4)
		vbox.add_child(flaws_section)

		# Get flaw lists
		var unfixed_flaws = game_manager.get_rocket_design_unfixed_flaw_names(index)
		var fixed_flaws = game_manager.get_rocket_design_fixed_flaw_names(index)

		if unfixed_flaws.size() == 0 and fixed_flaws.size() == 0:
			var no_flaws_label = Label.new()
			no_flaws_label.text = "No issues discovered yet"
			no_flaws_label.add_theme_font_size_override("font_size", 12)
			no_flaws_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
			flaws_section.add_child(no_flaws_label)
		else:
			# Show unfixed flaws (orange)
			for flaw_name in unfixed_flaws:
				var flaw_label = Label.new()
				flaw_label.text = "⚠ " + flaw_name
				flaw_label.add_theme_font_size_override("font_size", 12)
				flaw_label.add_theme_color_override("font_color", Color(1.0, 0.7, 0.3))
				flaws_section.add_child(flaw_label)

			# Show fixed flaws (green with strikethrough effect)
			for flaw_name in fixed_flaws:
				var flaw_label = Label.new()
				flaw_label.text = "✓ " + flaw_name + " (fixed)"
				flaw_label.add_theme_font_size_override("font_size", 12)
				flaw_label.add_theme_color_override("font_color", Color(0.4, 0.8, 0.4))
				flaws_section.add_child(flaw_label)

	# Add assign button
	var btn_hbox = HBoxContainer.new()
	btn_hbox.add_theme_constant_override("separation", 10)
	vbox.add_child(btn_hbox)

	var assign_btn = Button.new()
	assign_btn.text = "Assign Team"
	assign_btn.add_theme_font_size_override("font_size", 12)
	assign_btn.pressed.connect(_on_assign_team_pressed.bind(index))
	btn_hbox.add_child(assign_btn)

	if teams_count > 0:
		var unassign_btn = Button.new()
		unassign_btn.text = "Unassign Team"
		unassign_btn.add_theme_font_size_override("font_size", 12)
		unassign_btn.pressed.connect(_on_unassign_teams_pressed.bind(index))
		btn_hbox.add_child(unassign_btn)

	return panel

func _on_design_card_toggle(index: int):
	_expanded_design_cards[index] = not _expanded_design_cards.get(index, false)
	_update_research_designs()

func _get_teams_on_design(design_index: int) -> Array:
	var result = []
	var team_ids = game_manager.get_all_team_ids()
	for id in team_ids:
		var assignment = game_manager.get_team_assignment(id)
		if assignment.get("type") == "design" and assignment.get("design_index") == design_index:
			result.append(id)
	return result

func _update_research_engines():
	if not _research_engines_container:
		return

	# Clear existing
	for child in _research_engines_container.get_children():
		child.queue_free()

	var engine_count = game_manager.get_engine_type_count()
	for i in range(engine_count):
		var card = _create_engine_work_card(i)
		_research_engines_container.add_child(card)

func _create_engine_work_card(index: int) -> PanelContainer:
	var engine_name = game_manager.get_engine_type_name(index)
	var status = game_manager.get_engine_status(index)
	var base_status = game_manager.get_engine_status_base(index)
	var progress = game_manager.get_engine_progress(index)
	var teams_count = game_manager.get_teams_on_engine_count(index)
	var is_expanded = _expanded_engine_cards.get(index, false)

	var panel = PanelContainer.new()
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	# Style based on status
	var style = StyleBoxFlat.new()
	if base_status == "Untested":
		# Gray style for Untested
		style.set_bg_color(Color(0.1, 0.1, 0.1))
		style.set_border_color(Color(0.4, 0.4, 0.4, 0.5))
	elif base_status == "Refining":
		# Blue style for Refining
		style.set_bg_color(Color(0.08, 0.1, 0.18))
		style.set_border_color(Color(0.3, 0.5, 0.9, 0.5))
	elif base_status == "Fixing":
		# Orange style for Fixing
		style.set_bg_color(Color(0.18, 0.12, 0.08))
		style.set_border_color(Color(0.9, 0.6, 0.3, 0.5))
	else:
		style.set_bg_color(Color(0.08, 0.1, 0.15))
		style.set_border_color(Color(0.3, 0.5, 0.8, 0.5))
	style.set_border_width_all(2)
	style.set_corner_radius_all(4)
	panel.add_theme_stylebox_override("panel", style)

	var margin = MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 15)
	margin.add_theme_constant_override("margin_right", 15)
	margin.add_theme_constant_override("margin_top", 12)
	margin.add_theme_constant_override("margin_bottom", 12)
	panel.add_child(margin)

	var vbox = VBoxContainer.new()
	vbox.add_theme_constant_override("separation", 6)
	margin.add_child(vbox)

	var header = HBoxContainer.new()
	vbox.add_child(header)

	# Make engine name clickable to expand/collapse (only if not Untested)
	if base_status != "Untested":
		var name_btn = Button.new()
		name_btn.text = ("▼ " if is_expanded else "▶ ") + engine_name + " Engine"
		name_btn.add_theme_font_size_override("font_size", 18)
		name_btn.flat = true
		name_btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		name_btn.alignment = HORIZONTAL_ALIGNMENT_LEFT
		name_btn.pressed.connect(_on_engine_card_toggle.bind(index))
		header.add_child(name_btn)
	else:
		var name_label = Label.new()
		name_label.text = engine_name + " Engine"
		name_label.add_theme_font_size_override("font_size", 18)
		name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		header.add_child(name_label)

	var status_label = Label.new()
	status_label.text = status
	status_label.add_theme_font_size_override("font_size", 14)
	if base_status == "Untested":
		status_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	elif base_status == "Refining":
		status_label.add_theme_color_override("font_color", Color(0.4, 0.6, 1.0))
	elif base_status == "Fixing":
		status_label.add_theme_color_override("font_color", Color(1.0, 0.7, 0.3))
	header.add_child(status_label)

	# Testing level label (only when Refining or Fixing)
	if base_status == "Refining" or base_status == "Fixing":
		var testing_level = game_manager.get_engine_testing_level(index)
		var testing_level_name = game_manager.get_engine_testing_level_name(index)
		var testing_label = Label.new()
		testing_label.text = testing_level_name
		testing_label.add_theme_font_size_override("font_size", 12)
		testing_label.add_theme_color_override("font_color", _engine_testing_level_color(testing_level))
		vbox.add_child(testing_label)

	# Teams info (only if not Untested)
	if base_status != "Untested":
		if teams_count > 0:
			var teams_on = _get_teams_on_engine(index)
			for tid in teams_on:
				var tname_label = Label.new()
				tname_label.text = game_manager.get_team_name(tid)
				tname_label.add_theme_font_size_override("font_size", 12)
				tname_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
				vbox.add_child(tname_label)
		else:
			var teams_label = Label.new()
			teams_label.text = "No teams assigned"
			teams_label.add_theme_color_override("font_color", Color(0.8, 0.6, 0.3))
			teams_label.add_theme_font_size_override("font_size", 12)
			vbox.add_child(teams_label)

	# Progress bar - different styles for different phases
	if base_status == "Refining":
		# Refining: blue bar always at 100%
		var progress_bar = ProgressBar.new()
		progress_bar.value = 100
		progress_bar.custom_minimum_size = Vector2(0, 12)
		progress_bar.show_percentage = false
		var fill_style = StyleBoxFlat.new()
		fill_style.set_bg_color(Color(0.3, 0.5, 0.9))
		fill_style.set_corner_radius_all(3)
		progress_bar.add_theme_stylebox_override("fill", fill_style)
		vbox.add_child(progress_bar)
	elif base_status == "Fixing":
		# Fixing: orange progress bar
		var progress_bar = ProgressBar.new()
		progress_bar.value = progress * 100
		progress_bar.custom_minimum_size = Vector2(0, 12)
		progress_bar.show_percentage = true
		var fill_style = StyleBoxFlat.new()
		fill_style.set_bg_color(Color(0.9, 0.6, 0.3))
		fill_style.set_corner_radius_all(3)
		progress_bar.add_theme_stylebox_override("fill", fill_style)
		vbox.add_child(progress_bar)

	# Expanded section: show flaws (only if not Untested)
	if is_expanded and base_status != "Untested":
		var separator = HSeparator.new()
		vbox.add_child(separator)

		var flaws_section = VBoxContainer.new()
		flaws_section.add_theme_constant_override("separation", 4)
		vbox.add_child(flaws_section)

		# Get flaw lists
		var unfixed_flaws = game_manager.get_engine_unfixed_flaw_names(index)
		var fixed_flaws = game_manager.get_engine_fixed_flaw_names(index)

		if unfixed_flaws.size() == 0 and fixed_flaws.size() == 0:
			var no_flaws_label = Label.new()
			no_flaws_label.text = "No issues discovered yet"
			no_flaws_label.add_theme_font_size_override("font_size", 12)
			no_flaws_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
			flaws_section.add_child(no_flaws_label)
		else:
			# Show unfixed flaws (orange)
			for flaw_name in unfixed_flaws:
				var flaw_label = Label.new()
				flaw_label.text = "⚠ " + flaw_name
				flaw_label.add_theme_font_size_override("font_size", 12)
				flaw_label.add_theme_color_override("font_color", Color(1.0, 0.7, 0.3))
				flaws_section.add_child(flaw_label)

			# Show fixed flaws (green)
			for flaw_name in fixed_flaws:
				var flaw_label = Label.new()
				flaw_label.text = "✓ " + flaw_name + " (fixed)"
				flaw_label.add_theme_font_size_override("font_size", 12)
				flaw_label.add_theme_color_override("font_color", Color(0.4, 0.8, 0.4))
				flaws_section.add_child(flaw_label)

	# Buttons
	var btn_hbox = HBoxContainer.new()
	btn_hbox.add_theme_constant_override("separation", 10)
	vbox.add_child(btn_hbox)

	if base_status == "Untested":
		# Submit to Refining button
		var submit_btn = Button.new()
		submit_btn.text = "Submit to Refining"
		submit_btn.add_theme_font_size_override("font_size", 12)
		submit_btn.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
		submit_btn.pressed.connect(_on_submit_engine_pressed.bind(index))
		btn_hbox.add_child(submit_btn)
	else:
		var assign_btn = Button.new()
		assign_btn.text = "Assign Team"
		assign_btn.add_theme_font_size_override("font_size", 12)
		assign_btn.pressed.connect(_on_assign_engine_team_pressed.bind(index))
		btn_hbox.add_child(assign_btn)

		if teams_count > 0:
			var unassign_btn = Button.new()
			unassign_btn.text = "Unassign Team"
			unassign_btn.add_theme_font_size_override("font_size", 12)
			unassign_btn.pressed.connect(_on_unassign_engine_teams_pressed.bind(index))
			btn_hbox.add_child(unassign_btn)

	return panel

func _on_engine_card_toggle(index: int):
	_expanded_engine_cards[index] = not _expanded_engine_cards.get(index, false)
	_update_research_engines()

# Helper to get color for an engine testing level index (0-4)
func _engine_testing_level_color(level: int) -> Color:
	match level:
		0: return Color(1.0, 0.3, 0.3)       # Untested - Red
		1: return Color(1.0, 0.6, 0.2)       # Lightly Tested - Orange
		2: return Color(1.0, 1.0, 0.3)       # Moderately Tested - Yellow
		3: return Color(0.6, 1.0, 0.4)       # Well Tested - Light green
		4: return Color(0.3, 1.0, 0.3)       # Thoroughly Tested - Green
		_: return Color(0.5, 0.5, 0.5)

func _get_teams_on_engine(engine_index: int) -> Array:
	var result = []
	var team_ids = game_manager.get_all_team_ids()
	for id in team_ids:
		var assignment = game_manager.get_team_assignment(id)
		if assignment.get("type") == "engine" and assignment.get("engine_design_id") == engine_index:
			result.append(id)
	return result

func _on_submit_engine_pressed(index: int):
	game_manager.submit_engine_to_refining(index)
	_update_research_ui()

func _on_assign_engine_team_pressed(engine_index: int):
	# Find an available engineering team (unassigned, not ramping)
	var team_ids = game_manager.get_engineering_team_ids()
	for id in team_ids:
		var is_assigned = game_manager.is_team_assigned(id)
		var is_ramping = game_manager.is_team_ramping_up(id)
		if not is_assigned and not is_ramping:
			game_manager.assign_team_to_engine(id, engine_index)
			_update_research_ui()
			return

	# If no available team, try any unassigned engineering team (even if ramping)
	for id in team_ids:
		if not game_manager.is_team_assigned(id):
			game_manager.assign_team_to_engine(id, engine_index)
			_update_research_ui()
			return

	# No teams available - show a message
	_show_toast("No available engineering teams. Hire more!")

func _on_unassign_engine_teams_pressed(engine_index: int):
	# Unassign the last team from this engine
	var teams_on = _get_teams_on_engine(engine_index)
	if teams_on.size() > 0:
		game_manager.unassign_team(teams_on.back())
	_update_research_ui()

func _on_research_hire_pressed():
	var result = game_manager.hire_engineering_team()
	if result < 0:
		_show_toast("Cannot afford to hire ($150K)")

func _on_research_teams_changed():
	_update_research_teams()

func _on_research_designs_changed():
	_update_research_designs()

func _on_research_update_timer():
	# Periodic update of research UI (for progress bars and counters)
	if current_tab == Tab.RESEARCH:
		_update_research_ui()
	elif current_tab == Tab.PRODUCTION:
		_update_production_ui()

func _on_assign_team_pressed(design_index: int):
	# Find an available engineering team (unassigned, not ramping)
	var team_ids = game_manager.get_engineering_team_ids()
	for id in team_ids:
		var is_assigned = game_manager.is_team_assigned(id)
		var is_ramping = game_manager.is_team_ramping_up(id)
		if not is_assigned and not is_ramping:
			game_manager.assign_team_to_design(id, design_index)
			_update_research_ui()
			return

	# If no available team, try any unassigned engineering team (even if ramping)
	for id in team_ids:
		if not game_manager.is_team_assigned(id):
			game_manager.assign_team_to_design(id, design_index)
			_update_research_ui()
			return

	# No teams available - show a message
	_show_toast("No available engineering teams. Hire more!")

func _on_unassign_teams_pressed(design_index: int):
	# Unassign the last team from this design
	var teams_on = _get_teams_on_design(design_index)
	if teams_on.size() > 0:
		game_manager.unassign_team(teams_on.back())
	_update_research_ui()

# ==========================================
# Finance Tab
# ==========================================

func _setup_finance_content():
	var finance = content_areas[Tab.FINANCE]
	if not finance:
		return

	# Remove placeholder label
	for child in finance.get_children():
		child.queue_free()

	# Build the Finance UI
	var margin = MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", 20)
	margin.add_theme_constant_override("margin_top", 20)
	margin.add_theme_constant_override("margin_right", 20)
	margin.add_theme_constant_override("margin_bottom", 20)
	finance.add_child(margin)

	var scroll = ScrollContainer.new()
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	margin.add_child(scroll)

	var main_vbox = VBoxContainer.new()
	main_vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main_vbox.add_theme_constant_override("separation", 20)
	scroll.add_child(main_vbox)

	var title = Label.new()
	title.text = "Finance"
	title.add_theme_font_size_override("font_size", 24)
	main_vbox.add_child(title)

	# === Section 1: Financial Summary ===
	var summary_panel = PanelContainer.new()
	summary_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main_vbox.add_child(summary_panel)

	var summary_margin = MarginContainer.new()
	summary_margin.add_theme_constant_override("margin_left", 15)
	summary_margin.add_theme_constant_override("margin_top", 15)
	summary_margin.add_theme_constant_override("margin_right", 15)
	summary_margin.add_theme_constant_override("margin_bottom", 15)
	summary_panel.add_child(summary_margin)

	var summary_vbox = VBoxContainer.new()
	summary_vbox.add_theme_constant_override("separation", 10)
	summary_margin.add_child(summary_vbox)

	var summary_title = Label.new()
	summary_title.text = "Financial Summary"
	summary_title.add_theme_font_size_override("font_size", 20)
	summary_vbox.add_child(summary_title)

	_finance_balance_label = Label.new()
	_finance_balance_label.add_theme_font_size_override("font_size", 28)
	_finance_balance_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
	summary_vbox.add_child(_finance_balance_label)

	_finance_burn_label = Label.new()
	_finance_burn_label.add_theme_font_size_override("font_size", 16)
	_finance_burn_label.add_theme_color_override("font_color", Color(1, 0.85, 0.3))
	summary_vbox.add_child(_finance_burn_label)

	_finance_salary_date_label = Label.new()
	_finance_salary_date_label.add_theme_font_size_override("font_size", 14)
	_finance_salary_date_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
	summary_vbox.add_child(_finance_salary_date_label)

	_finance_runway_label = Label.new()
	_finance_runway_label.add_theme_font_size_override("font_size", 16)
	summary_vbox.add_child(_finance_runway_label)

	# === Section 2: Team Payroll ===
	var payroll_panel = PanelContainer.new()
	payroll_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main_vbox.add_child(payroll_panel)

	var payroll_margin = MarginContainer.new()
	payroll_margin.add_theme_constant_override("margin_left", 15)
	payroll_margin.add_theme_constant_override("margin_top", 15)
	payroll_margin.add_theme_constant_override("margin_right", 15)
	payroll_margin.add_theme_constant_override("margin_bottom", 15)
	payroll_panel.add_child(payroll_margin)

	var payroll_vbox = VBoxContainer.new()
	payroll_vbox.add_theme_constant_override("separation", 10)
	payroll_margin.add_child(payroll_vbox)

	var payroll_title = Label.new()
	payroll_title.text = "Team Payroll"
	payroll_title.add_theme_font_size_override("font_size", 20)
	payroll_vbox.add_child(payroll_title)

	# Engineering teams sub-section
	_finance_eng_header_label = Label.new()
	_finance_eng_header_label.add_theme_font_size_override("font_size", 16)
	_finance_eng_header_label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
	payroll_vbox.add_child(_finance_eng_header_label)

	_finance_eng_teams_container = VBoxContainer.new()
	_finance_eng_teams_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_finance_eng_teams_container.add_theme_constant_override("separation", 4)
	payroll_vbox.add_child(_finance_eng_teams_container)

	# Manufacturing teams sub-section
	_finance_mfg_header_label = Label.new()
	_finance_mfg_header_label.add_theme_font_size_override("font_size", 16)
	_finance_mfg_header_label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
	payroll_vbox.add_child(_finance_mfg_header_label)

	_finance_mfg_teams_container = VBoxContainer.new()
	_finance_mfg_teams_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_finance_mfg_teams_container.add_theme_constant_override("separation", 4)
	payroll_vbox.add_child(_finance_mfg_teams_container)

	# === Section 3: Reference Prices ===
	var prices_panel = PanelContainer.new()
	prices_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main_vbox.add_child(prices_panel)

	var prices_margin = MarginContainer.new()
	prices_margin.add_theme_constant_override("margin_left", 15)
	prices_margin.add_theme_constant_override("margin_top", 15)
	prices_margin.add_theme_constant_override("margin_right", 15)
	prices_margin.add_theme_constant_override("margin_bottom", 15)
	prices_panel.add_child(prices_margin)

	var prices_vbox = VBoxContainer.new()
	prices_vbox.add_theme_constant_override("separation", 10)
	prices_margin.add_child(prices_vbox)

	var prices_title = Label.new()
	prices_title.text = "Reference Prices"
	prices_title.add_theme_font_size_override("font_size", 20)
	prices_vbox.add_child(prices_title)

	_finance_prices_container = VBoxContainer.new()
	_finance_prices_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_finance_prices_container.add_theme_constant_override("separation", 6)
	prices_vbox.add_child(_finance_prices_container)

	# Connect teams_changed to refresh finance when visible
	game_manager.teams_changed.connect(_on_finance_teams_changed)

func _on_finance_teams_changed():
	if current_tab == Tab.FINANCE:
		_update_finance_ui()

func _update_finance_ui():
	if not _finance_balance_label:
		return

	# === Financial Summary ===
	_finance_balance_label.text = "Balance: %s" % game_manager.get_money_formatted()

	var monthly_burn = game_manager.get_total_monthly_salary()
	_finance_burn_label.text = "Monthly burn: %s/mo" % _format_money_value(monthly_burn)

	var days_to_salary = game_manager.days_until_salary()
	_finance_salary_date_label.text = "Next salary payment in %d day%s" % [days_to_salary, "s" if days_to_salary != 1 else ""]

	# Runway calculation
	if monthly_burn > 0:
		var balance = game_manager.get_money()
		var runway_months = balance / monthly_burn
		_finance_runway_label.text = "Runway: %.1f months" % runway_months
		if runway_months > 12.0:
			_finance_runway_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))
		elif runway_months > 6.0:
			_finance_runway_label.add_theme_color_override("font_color", Color(1, 0.85, 0.3))
		else:
			_finance_runway_label.add_theme_color_override("font_color", Color(1.0, 0.3, 0.3))
	else:
		_finance_runway_label.text = "Runway: No expenses"
		_finance_runway_label.add_theme_color_override("font_color", Color(0.3, 1.0, 0.3))

	# === Team Payroll ===
	_update_finance_payroll()

	# === Reference Prices ===
	_update_finance_prices()

func _update_finance_payroll():
	var eng_salary_str = _format_money_value(game_manager.get_engineering_team_salary())
	var mfg_salary_str = _format_money_value(game_manager.get_manufacturing_team_salary())

	# Engineering teams
	var eng_ids = game_manager.get_engineering_team_ids()
	var eng_total = game_manager.get_engineering_monthly_salary()
	_finance_eng_header_label.text = "Engineering Teams (%d) — %s/mo" % [eng_ids.size(), _format_money_value(eng_total)]

	for child in _finance_eng_teams_container.get_children():
		child.queue_free()

	for id in eng_ids:
		var row = _create_finance_team_row(id, eng_salary_str)
		_finance_eng_teams_container.add_child(row)

	if eng_ids.size() == 0:
		var none_label = Label.new()
		none_label.text = "  No engineering teams hired"
		none_label.add_theme_font_size_override("font_size", 13)
		none_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		_finance_eng_teams_container.add_child(none_label)

	# Manufacturing teams
	var mfg_ids = game_manager.get_manufacturing_team_ids()
	var mfg_total = game_manager.get_manufacturing_monthly_salary()
	_finance_mfg_header_label.text = "Manufacturing Teams (%d) — %s/mo" % [mfg_ids.size(), _format_money_value(mfg_total)]

	for child in _finance_mfg_teams_container.get_children():
		child.queue_free()

	for id in mfg_ids:
		var row = _create_finance_team_row(id, mfg_salary_str)
		_finance_mfg_teams_container.add_child(row)

	if mfg_ids.size() == 0:
		var none_label = Label.new()
		none_label.text = "  No manufacturing teams hired"
		none_label.add_theme_font_size_override("font_size", 13)
		none_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		_finance_mfg_teams_container.add_child(none_label)

func _create_finance_team_row(team_id: int, salary_str: String) -> HBoxContainer:
	var hbox = HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 10)

	var name_label = Label.new()
	name_label.text = "  %s" % game_manager.get_team_name(team_id)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.add_theme_font_size_override("font_size", 13)
	hbox.add_child(name_label)

	var cost_label = Label.new()
	cost_label.text = "%s/mo" % salary_str
	cost_label.add_theme_font_size_override("font_size", 13)
	cost_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
	hbox.add_child(cost_label)

	return hbox

func _update_finance_prices():
	for child in _finance_prices_container.get_children():
		child.queue_free()

	# Team hiring costs
	_add_price_row("Engineering team hire", _format_money_value(game_manager.get_engineering_hire_cost()))
	_add_price_row("Manufacturing team hire", _format_money_value(game_manager.get_manufacturing_hire_cost()))
	_add_price_row("Engineering salary", "%s/mo" % _format_money_value(game_manager.get_engineering_team_salary()))
	_add_price_row("Manufacturing salary", "%s/mo" % _format_money_value(game_manager.get_manufacturing_team_salary()))
	_add_price_row("Floor space (per unit)", _format_money_value(game_manager.get_floor_space_cost_per_unit()))

	# Resource prices
	for i in range(game_manager.get_resource_count()):
		_add_price_row(game_manager.get_resource_name(i),
			"%s/kg" % _format_money_value(game_manager.get_resource_price(i)))

	# Other costs
	var pad_cost = game_manager.get_pad_upgrade_cost()
	if pad_cost > 0:
		_add_price_row("Next pad upgrade", _format_money_value(pad_cost))
	else:
		_add_price_row("Pad upgrade", "Max level")

	_add_price_row("Contract refresh", _format_money_value(game_manager.get_refresh_cost()))

func _add_price_row(item_name: String, price_str: String):
	var hbox = HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 10)

	var name_label = Label.new()
	name_label.text = item_name
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.add_theme_font_size_override("font_size", 14)
	hbox.add_child(name_label)

	var value_label = Label.new()
	value_label.text = price_str
	value_label.add_theme_font_size_override("font_size", 14)
	value_label.add_theme_color_override("font_color", Color(1, 0.85, 0.3))
	hbox.add_child(value_label)

	_finance_prices_container.add_child(hbox)

func _format_money_value(value: float) -> String:
	if value >= 1_000_000_000.0:
		return "$%.1fB" % (value / 1_000_000_000.0)
	elif value >= 1_000_000.0:
		return "$%.1fM" % (value / 1_000_000.0)
	elif value >= 1_000.0:
		return "$%.0fK" % (value / 1_000.0)
	else:
		return "$%.0f" % value

# ==========================================
# Production Tab
# ==========================================

func _setup_production_content():
	var production = content_areas[Tab.PRODUCTION]
	if not production:
		return

	# Remove placeholder label
	for child in production.get_children():
		child.queue_free()

	# Build the Production UI
	var margin = MarginContainer.new()
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	margin.add_theme_constant_override("margin_left", 20)
	margin.add_theme_constant_override("margin_top", 20)
	margin.add_theme_constant_override("margin_right", 20)
	margin.add_theme_constant_override("margin_bottom", 20)
	production.add_child(margin)

	var scroll = ScrollContainer.new()
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	margin.add_child(scroll)

	var main_vbox = VBoxContainer.new()
	main_vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main_vbox.add_theme_constant_override("separation", 20)
	scroll.add_child(main_vbox)

	var title = Label.new()
	title.text = "Production"
	title.add_theme_font_size_override("font_size", 24)
	main_vbox.add_child(title)

	# === Manufacturing Teams Section ===
	var mfg_teams_panel = PanelContainer.new()
	mfg_teams_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main_vbox.add_child(mfg_teams_panel)

	var mfg_margin = MarginContainer.new()
	mfg_margin.add_theme_constant_override("margin_left", 15)
	mfg_margin.add_theme_constant_override("margin_top", 15)
	mfg_margin.add_theme_constant_override("margin_right", 15)
	mfg_margin.add_theme_constant_override("margin_bottom", 15)
	mfg_teams_panel.add_child(mfg_margin)

	var mfg_vbox = VBoxContainer.new()
	mfg_vbox.add_theme_constant_override("separation", 12)
	mfg_margin.add_child(mfg_vbox)

	var mfg_title = Label.new()
	mfg_title.text = "Manufacturing Teams"
	mfg_title.add_theme_font_size_override("font_size", 20)
	mfg_vbox.add_child(mfg_title)

	# Team count + hire row
	var mfg_header = HBoxContainer.new()
	mfg_header.add_theme_constant_override("separation", 10)
	mfg_vbox.add_child(mfg_header)

	_prod_mfg_team_count_label = Label.new()
	_prod_mfg_team_count_label.text = "Teams: 0"
	_prod_mfg_team_count_label.add_theme_font_size_override("font_size", 14)
	mfg_header.add_child(_prod_mfg_team_count_label)

	var hire_mfg_btn = Button.new()
	hire_mfg_btn.text = "+ Hire Mfg Team ($450K)"
	hire_mfg_btn.add_theme_font_size_override("font_size", 14)
	hire_mfg_btn.pressed.connect(_on_prod_hire_mfg_pressed)
	mfg_header.add_child(hire_mfg_btn)

	_prod_mfg_salary_label = Label.new()
	_prod_mfg_salary_label.text = "Monthly salary: $0"
	_prod_mfg_salary_label.add_theme_font_size_override("font_size", 12)
	_prod_mfg_salary_label.add_theme_color_override("font_color", Color(1, 0.85, 0.3))
	mfg_vbox.add_child(_prod_mfg_salary_label)

	_prod_mfg_teams_container = VBoxContainer.new()
	_prod_mfg_teams_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_prod_mfg_teams_container.add_theme_constant_override("separation", 8)
	mfg_vbox.add_child(_prod_mfg_teams_container)

	# === Floor Space Section ===
	var floor_panel = PanelContainer.new()
	floor_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main_vbox.add_child(floor_panel)

	var floor_margin = MarginContainer.new()
	floor_margin.add_theme_constant_override("margin_left", 15)
	floor_margin.add_theme_constant_override("margin_top", 15)
	floor_margin.add_theme_constant_override("margin_right", 15)
	floor_margin.add_theme_constant_override("margin_bottom", 15)
	floor_panel.add_child(floor_margin)

	var floor_vbox = VBoxContainer.new()
	floor_vbox.add_theme_constant_override("separation", 12)
	floor_margin.add_child(floor_vbox)

	var floor_title = Label.new()
	floor_title.text = "Floor Space"
	floor_title.add_theme_font_size_override("font_size", 18)
	floor_vbox.add_child(floor_title)

	var floor_hbox = HBoxContainer.new()
	floor_hbox.add_theme_constant_override("separation", 15)
	floor_vbox.add_child(floor_hbox)

	_prod_floor_space_label = Label.new()
	_prod_floor_space_label.add_theme_font_size_override("font_size", 14)
	_prod_floor_space_label.add_theme_color_override("font_color", Color(0.4, 0.8, 1.0))
	_prod_floor_space_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	floor_hbox.add_child(_prod_floor_space_label)

	var buy_space_btn = Button.new()
	buy_space_btn.text = "Buy Floor Space ($5M/unit)"
	buy_space_btn.add_theme_font_size_override("font_size", 13)
	buy_space_btn.pressed.connect(_on_prod_buy_floor_space)
	floor_hbox.add_child(buy_space_btn)

	# === New Order Buttons ===
	var order_btns_hbox = HBoxContainer.new()
	order_btns_hbox.add_theme_constant_override("separation", 15)
	main_vbox.add_child(order_btns_hbox)

	var build_engines_btn = Button.new()
	build_engines_btn.text = "Build Engines..."
	build_engines_btn.add_theme_font_size_override("font_size", 14)
	build_engines_btn.pressed.connect(_on_prod_build_engines_pressed)
	order_btns_hbox.add_child(build_engines_btn)

	var assemble_rocket_btn = Button.new()
	assemble_rocket_btn.text = "Assemble Rocket..."
	assemble_rocket_btn.add_theme_font_size_override("font_size", 14)
	assemble_rocket_btn.pressed.connect(_on_prod_assemble_rocket_pressed)
	order_btns_hbox.add_child(assemble_rocket_btn)

	var auto_assign_btn = Button.new()
	auto_assign_btn.text = "Auto-Assign Teams"
	auto_assign_btn.add_theme_font_size_override("font_size", 14)
	auto_assign_btn.pressed.connect(_on_prod_auto_assign_teams)
	order_btns_hbox.add_child(auto_assign_btn)

	# === Manufacturing Queue Section ===
	var queue_panel = PanelContainer.new()
	queue_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	main_vbox.add_child(queue_panel)

	var queue_margin = MarginContainer.new()
	queue_margin.add_theme_constant_override("margin_left", 15)
	queue_margin.add_theme_constant_override("margin_top", 15)
	queue_margin.add_theme_constant_override("margin_right", 15)
	queue_margin.add_theme_constant_override("margin_bottom", 15)
	queue_panel.add_child(queue_margin)

	var queue_vbox = VBoxContainer.new()
	queue_vbox.add_theme_constant_override("separation", 10)
	queue_margin.add_child(queue_vbox)

	var queue_title = Label.new()
	queue_title.text = "Manufacturing Queue"
	queue_title.add_theme_font_size_override("font_size", 18)
	queue_vbox.add_child(queue_title)

	_prod_no_orders_label = Label.new()
	_prod_no_orders_label.text = "No active manufacturing orders. Build engines or assemble rockets above."
	_prod_no_orders_label.add_theme_font_size_override("font_size", 14)
	_prod_no_orders_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
	_prod_no_orders_label.autowrap_mode = TextServer.AUTOWRAP_WORD
	queue_vbox.add_child(_prod_no_orders_label)

	_prod_orders_container = VBoxContainer.new()
	_prod_orders_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_prod_orders_container.add_theme_constant_override("separation", 10)
	queue_vbox.add_child(_prod_orders_container)

	# === Inventory Sections ===
	var inv_hbox = HBoxContainer.new()
	inv_hbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	inv_hbox.add_theme_constant_override("separation", 20)
	main_vbox.add_child(inv_hbox)

	# Engine Inventory
	var engine_inv_panel = PanelContainer.new()
	engine_inv_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	inv_hbox.add_child(engine_inv_panel)

	var engine_inv_margin = MarginContainer.new()
	engine_inv_margin.add_theme_constant_override("margin_left", 15)
	engine_inv_margin.add_theme_constant_override("margin_top", 15)
	engine_inv_margin.add_theme_constant_override("margin_right", 15)
	engine_inv_margin.add_theme_constant_override("margin_bottom", 15)
	engine_inv_panel.add_child(engine_inv_margin)

	var engine_inv_vbox = VBoxContainer.new()
	engine_inv_vbox.add_theme_constant_override("separation", 8)
	engine_inv_margin.add_child(engine_inv_vbox)

	var engine_inv_title = Label.new()
	engine_inv_title.text = "Engine Inventory"
	engine_inv_title.add_theme_font_size_override("font_size", 18)
	engine_inv_vbox.add_child(engine_inv_title)

	_prod_engine_inv_container = VBoxContainer.new()
	_prod_engine_inv_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_prod_engine_inv_container.add_theme_constant_override("separation", 4)
	engine_inv_vbox.add_child(_prod_engine_inv_container)

	# Rocket Inventory
	var rocket_inv_panel = PanelContainer.new()
	rocket_inv_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	inv_hbox.add_child(rocket_inv_panel)

	var rocket_inv_margin = MarginContainer.new()
	rocket_inv_margin.add_theme_constant_override("margin_left", 15)
	rocket_inv_margin.add_theme_constant_override("margin_top", 15)
	rocket_inv_margin.add_theme_constant_override("margin_right", 15)
	rocket_inv_margin.add_theme_constant_override("margin_bottom", 15)
	rocket_inv_panel.add_child(rocket_inv_margin)

	var rocket_inv_vbox = VBoxContainer.new()
	rocket_inv_vbox.add_theme_constant_override("separation", 8)
	rocket_inv_margin.add_child(rocket_inv_vbox)

	var rocket_inv_title = Label.new()
	rocket_inv_title.text = "Rocket Inventory"
	rocket_inv_title.add_theme_font_size_override("font_size", 18)
	rocket_inv_vbox.add_child(rocket_inv_title)

	_prod_rocket_inv_container = VBoxContainer.new()
	_prod_rocket_inv_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_prod_rocket_inv_container.add_theme_constant_override("separation", 4)
	rocket_inv_vbox.add_child(_prod_rocket_inv_container)

	# Initial update
	_update_production_ui()

func _update_production_ui():
	_update_production_mfg_teams()
	_update_production_floor_space()
	_update_production_orders()
	_update_production_inventory()

func _update_production_mfg_teams():
	if not _prod_mfg_teams_container:
		return

	var mfg_ids = game_manager.get_manufacturing_team_ids()
	if _prod_mfg_team_count_label:
		_prod_mfg_team_count_label.text = "Teams: %d" % mfg_ids.size()
	if _prod_mfg_salary_label:
		var salary = game_manager.get_manufacturing_monthly_salary()
		_prod_mfg_salary_label.text = "Monthly salary: $%.0fK" % (salary / 1000.0)

	for child in _prod_mfg_teams_container.get_children():
		child.queue_free()

	for id in mfg_ids:
		var card = _create_team_card(id)
		_prod_mfg_teams_container.add_child(card)

func _update_production_floor_space():
	if not _prod_floor_space_label:
		return

	var total = game_manager.get_floor_space_total()
	var in_use = game_manager.get_floor_space_in_use()
	var available = game_manager.get_floor_space_available()
	var constructing = game_manager.get_floor_space_under_construction()

	var text = "%d / %d units in use (%d available)" % [in_use, total, available]
	if constructing > 0:
		text += " [%d under construction]" % constructing
	_prod_floor_space_label.text = text

func _update_production_orders():
	if not _prod_orders_container:
		return

	# Clear existing
	for child in _prod_orders_container.get_children():
		child.queue_free()

	var order_ids = game_manager.get_active_order_ids()
	_prod_no_orders_label.visible = order_ids.size() == 0

	for order_id in order_ids:
		var card = _create_order_card(order_id)
		_prod_orders_container.add_child(card)

func _create_order_card(order_id: int) -> PanelContainer:
	var info = game_manager.get_order_info(order_id)
	var display_name = info.get("display_name", "Unknown")
	var is_engine = info.get("is_engine", false)
	var progress = info.get("progress", 0.0)
	var teams_count = game_manager.get_teams_on_order_count(order_id)

	var panel = PanelContainer.new()
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	var style = StyleBoxFlat.new()
	if is_engine:
		style.set_bg_color(Color(0.1, 0.08, 0.15))
		style.set_border_color(Color(0.6, 0.4, 0.9, 0.5))
	else:
		style.set_bg_color(Color(0.08, 0.12, 0.15))
		style.set_border_color(Color(0.3, 0.7, 0.9, 0.5))
	style.set_border_width_all(2)
	style.set_corner_radius_all(4)
	panel.add_theme_stylebox_override("panel", style)

	var card_margin = MarginContainer.new()
	card_margin.add_theme_constant_override("margin_left", 15)
	card_margin.add_theme_constant_override("margin_right", 15)
	card_margin.add_theme_constant_override("margin_top", 10)
	card_margin.add_theme_constant_override("margin_bottom", 10)
	panel.add_child(card_margin)

	var vbox = VBoxContainer.new()
	vbox.add_theme_constant_override("separation", 6)
	card_margin.add_child(vbox)

	# Header
	var header = HBoxContainer.new()
	vbox.add_child(header)

	var name_label = Label.new()
	name_label.text = display_name
	name_label.add_theme_font_size_override("font_size", 16)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(name_label)

	# Engine orders show completed/quantity
	if is_engine:
		var completed = info.get("completed", 0)
		var quantity = info.get("quantity", 1)
		var remaining = quantity - completed
		var qty_label = Label.new()
		qty_label.text = "%d remaining" % remaining
		qty_label.add_theme_font_size_override("font_size", 14)
		qty_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		header.add_child(qty_label)

	# Teams info
	if teams_count > 0:
		var teams_on = _get_teams_on_order(order_id)
		for tid in teams_on:
			var tname_label = Label.new()
			tname_label.text = game_manager.get_team_name(tid)
			tname_label.add_theme_font_size_override("font_size", 12)
			tname_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
			vbox.add_child(tname_label)
	else:
		var teams_label = Label.new()
		teams_label.text = "No teams assigned - assign teams to start production"
		teams_label.add_theme_color_override("font_color", Color(0.8, 0.6, 0.3))
		teams_label.add_theme_font_size_override("font_size", 12)
		vbox.add_child(teams_label)

	# Progress bar
	var progress_bar = ProgressBar.new()
	progress_bar.value = progress * 100
	progress_bar.custom_minimum_size = Vector2(0, 12)
	progress_bar.show_percentage = true
	var fill_style = StyleBoxFlat.new()
	if is_engine:
		fill_style.set_bg_color(Color(0.6, 0.4, 0.9))
	else:
		fill_style.set_bg_color(Color(0.3, 0.7, 0.9))
	fill_style.set_corner_radius_all(3)
	progress_bar.add_theme_stylebox_override("fill", fill_style)
	vbox.add_child(progress_bar)

	# Buttons
	var btn_hbox = HBoxContainer.new()
	btn_hbox.add_theme_constant_override("separation", 10)
	vbox.add_child(btn_hbox)

	var assign_btn = Button.new()
	assign_btn.text = "Assign Team"
	assign_btn.add_theme_font_size_override("font_size", 12)
	assign_btn.pressed.connect(_on_prod_assign_team_pressed.bind(order_id))
	btn_hbox.add_child(assign_btn)

	if teams_count > 0:
		var unassign_btn = Button.new()
		unassign_btn.text = "Unassign Team"
		unassign_btn.add_theme_font_size_override("font_size", 12)
		unassign_btn.pressed.connect(_on_prod_unassign_teams_pressed.bind(order_id))
		btn_hbox.add_child(unassign_btn)

	var cancel_btn = Button.new()
	cancel_btn.text = "Cancel"
	cancel_btn.add_theme_font_size_override("font_size", 12)
	cancel_btn.add_theme_color_override("font_color", Color(1.0, 0.4, 0.4))
	cancel_btn.pressed.connect(_on_prod_cancel_order_pressed.bind(order_id))
	btn_hbox.add_child(cancel_btn)

	return panel

func _update_production_inventory():
	if not _prod_engine_inv_container:
		return

	# Engine inventory
	for child in _prod_engine_inv_container.get_children():
		child.queue_free()

	var engine_inv = game_manager.get_engine_inventory()
	if engine_inv.size() == 0:
		var empty_label = Label.new()
		empty_label.text = "No engines in stock"
		empty_label.add_theme_font_size_override("font_size", 13)
		empty_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		_prod_engine_inv_container.add_child(empty_label)
	else:
		for entry in engine_inv:
			var entry_name = entry.get("name", "Unknown")
			var qty = entry.get("quantity", 0)
			var rev = entry.get("revision_number", 0)
			var entry_label = Label.new()
			entry_label.text = "%s (Rev %d): %d in stock" % [entry_name, rev, qty]
			entry_label.add_theme_font_size_override("font_size", 13)
			entry_label.add_theme_color_override("font_color", Color(0.7, 0.8, 1.0))
			_prod_engine_inv_container.add_child(entry_label)

	# Rocket inventory
	for child in _prod_rocket_inv_container.get_children():
		child.queue_free()

	var rocket_inv = game_manager.get_rocket_inventory()
	if rocket_inv.size() == 0:
		var empty_label = Label.new()
		empty_label.text = "No assembled rockets"
		empty_label.add_theme_font_size_override("font_size", 13)
		empty_label.add_theme_color_override("font_color", Color(0.5, 0.5, 0.5))
		_prod_rocket_inv_container.add_child(empty_label)
	else:
		for entry in rocket_inv:
			var entry_name = entry.get("name", "Unknown")
			var serial = entry.get("serial_number", 0)
			var entry_label = Label.new()
			entry_label.text = "%s (S/N %d)" % [entry_name, serial]
			entry_label.add_theme_font_size_override("font_size", 13)
			entry_label.add_theme_color_override("font_color", Color(0.5, 0.9, 0.7))
			_prod_rocket_inv_container.add_child(entry_label)

# Production tab action handlers

func _on_prod_hire_mfg_pressed():
	var result = game_manager.hire_manufacturing_team()
	if result < 0:
		_show_toast("Cannot afford to hire ($450K)")
	else:
		_update_production_ui()

func _on_prod_buy_floor_space():
	# Show a small dialog to select how many units
	var dialog = AcceptDialog.new()
	dialog.title = "Buy Floor Space"
	dialog.size = Vector2(300, 150)

	var vbox = VBoxContainer.new()
	vbox.add_theme_constant_override("separation", 10)
	dialog.add_child(vbox)

	var info_label = Label.new()
	info_label.text = "Cost: $5M per unit (30 days to build)"
	info_label.add_theme_font_size_override("font_size", 13)
	vbox.add_child(info_label)

	var spin = SpinBox.new()
	spin.min_value = 1
	spin.max_value = 20
	spin.value = 1
	spin.step = 1
	vbox.add_child(spin)

	add_child(dialog)
	dialog.popup_centered()

	dialog.confirmed.connect(func():
		var units = int(spin.value)
		var success = game_manager.buy_floor_space(units)
		if success:
			_show_toast("Floor space ordered: %d units (30 days)" % units)
			_update_production_ui()
		else:
			_show_toast("Cannot afford floor space")
		dialog.queue_free()
	)
	dialog.canceled.connect(func(): dialog.queue_free())

func _on_prod_assign_team_pressed(order_id: int):
	# Find an available manufacturing team
	var team_ids = game_manager.get_manufacturing_team_ids()
	for id in team_ids:
		var is_assigned = game_manager.is_team_assigned(id)
		var is_ramping = game_manager.is_team_ramping_up(id)
		if not is_assigned and not is_ramping:
			game_manager.assign_team_to_manufacturing(id, order_id)
			_update_production_ui()
			_update_research_teams()
			return

	# If no available team, try any unassigned manufacturing team
	for id in team_ids:
		if not game_manager.is_team_assigned(id):
			game_manager.assign_team_to_manufacturing(id, order_id)
			_update_production_ui()
			_update_research_teams()
			return

	_show_toast("No available manufacturing teams. Hire more!")

func _get_teams_on_order(order_id: int) -> Array:
	var result = []
	var team_ids = game_manager.get_all_team_ids()
	for id in team_ids:
		var assignment = game_manager.get_team_assignment(id)
		if assignment.get("type") == "manufacturing" and assignment.get("order_id") == order_id:
			result.append(id)
	return result

func _on_prod_unassign_teams_pressed(order_id: int):
	# Unassign the last team from this order
	var teams_on = _get_teams_on_order(order_id)
	if teams_on.size() > 0:
		game_manager.unassign_team(teams_on.back())
	_update_production_ui()
	_update_research_teams()

func _on_prod_cancel_order_pressed(order_id: int):
	game_manager.cancel_manufacturing_order(order_id)
	_update_production_ui()
	_update_research_teams()

func _on_prod_build_engines_pressed():
	# Show dialog to select which engine to build
	var engine_count = game_manager.get_engine_type_count()
	if engine_count == 0:
		_show_toast("No engine designs available")
		return

	var dialog = AcceptDialog.new()
	dialog.title = "Build Engines"
	dialog.size = Vector2(450, 400)

	var dialog_vbox = VBoxContainer.new()
	dialog_vbox.add_theme_constant_override("separation", 10)
	dialog.add_child(dialog_vbox)

	var instructions = Label.new()
	instructions.text = "Select an engine design to manufacture:"
	instructions.add_theme_font_size_override("font_size", 14)
	dialog_vbox.add_child(instructions)

	for i in range(engine_count):
		var engine_name = game_manager.get_engine_type_name(i)
		var material_cost = game_manager.get_engine_material_cost(i)
		var build_days = game_manager.get_engine_build_days(i)

		var btn = Button.new()
		btn.text = "%s - $%sM material, ~%.0f team-days" % [engine_name, _format_money_short(material_cost / 1_000_000.0), build_days]
		btn.add_theme_font_size_override("font_size", 13)
		btn.pressed.connect(_on_prod_engine_selected.bind(i, dialog))
		dialog_vbox.add_child(btn)

	add_child(dialog)
	dialog.popup_centered()

func _on_prod_engine_selected(engine_index: int, dialog: AcceptDialog):
	dialog.queue_free()

	# Show quantity dialog
	var qty_dialog = AcceptDialog.new()
	qty_dialog.title = "Quantity"
	qty_dialog.size = Vector2(300, 150)

	var qty_vbox = VBoxContainer.new()
	qty_vbox.add_theme_constant_override("separation", 10)
	qty_dialog.add_child(qty_vbox)

	var qty_label = Label.new()
	qty_label.text = "How many to build?"
	qty_vbox.add_child(qty_label)

	var qty_spin = SpinBox.new()
	qty_spin.min_value = 1
	qty_spin.max_value = 50
	qty_spin.value = 1
	qty_spin.step = 1
	qty_vbox.add_child(qty_spin)

	add_child(qty_dialog)
	qty_dialog.popup_centered()

	qty_dialog.confirmed.connect(func():
		var quantity = int(qty_spin.value)
		# Need to cut a revision first
		var rev = game_manager.cut_engine_revision(engine_index, "mfg")
		if rev < 0:
			_show_toast("Failed to cut engine revision")
			qty_dialog.queue_free()
			return
		var result = game_manager.start_engine_order(engine_index, rev, quantity)
		if result >= 0:
			_show_toast("Engine order started!")
			_update_production_ui()
		else:
			_show_toast("Cannot start engine order: %s" % game_manager.get_last_order_error())
		qty_dialog.queue_free()
	)
	qty_dialog.canceled.connect(func(): qty_dialog.queue_free())

func _on_prod_assemble_rocket_pressed():
	# Show dialog to select which rocket to assemble
	var design_count = game_manager.get_rocket_design_count()
	if design_count == 0:
		_show_toast("No rocket designs available")
		return

	var dialog = AcceptDialog.new()
	dialog.title = "Assemble Rocket"
	dialog.size = Vector2(500, 400)

	var dialog_vbox = VBoxContainer.new()
	dialog_vbox.add_theme_constant_override("separation", 10)
	dialog.add_child(dialog_vbox)

	var instructions = Label.new()
	instructions.text = "Select a rocket design to assemble:"
	instructions.add_theme_font_size_override("font_size", 14)
	dialog_vbox.add_child(instructions)

	for i in range(design_count):
		var design_name = game_manager.get_rocket_design_name(i)
		var material_cost = game_manager.get_rocket_material_cost(i)
		var assembly_days = game_manager.get_rocket_assembly_days(i)
		var engines_req = game_manager.get_engines_required_for_rocket(i)

		var btn_text = "%s - $%sM material, ~%.0f team-days" % [design_name, _format_money_short(material_cost / 1_000_000.0), assembly_days]

		# Check if we have sufficient engines
		var has_engines = game_manager.has_engines_for_rocket(i)
		var missing_engines = game_manager.get_missing_engines_for_rocket(i)
		var total_deficit = 0
		for m in missing_engines:
			total_deficit += m.get("deficit", 0)

		if not has_engines:
			btn_text += " [AUTO-BUILD %d ENGINE%s]" % [total_deficit, "S" if total_deficit != 1 else ""]

		var btn = Button.new()
		btn.text = btn_text
		btn.add_theme_font_size_override("font_size", 13)
		if has_engines:
			btn.pressed.connect(_on_prod_rocket_selected.bind(i, dialog))
		else:
			btn.pressed.connect(_on_prod_auto_build_engines.bind(i, dialog))
		dialog_vbox.add_child(btn)

		# Show engines required with inventory counts
		if engines_req.size() > 0:
			var eng_info = RichTextLabel.new()
			eng_info.bbcode_enabled = true
			eng_info.fit_content = true
			eng_info.scroll_active = false
			eng_info.add_theme_font_size_override("normal_font_size", 11)
			var parts = []
			for req in engines_req:
				var eng_name = req.get("name", "?")
				var eng_count = req.get("count", 0)
				var eng_design_id = req.get("engine_design_id", -1)
				var in_stock = game_manager.get_engines_available_for_design(eng_design_id)
				var stock_color = "green" if in_stock >= eng_count else "red"
				parts.append("%dx %s ([color=%s]%d in stock[/color])" % [eng_count, eng_name, stock_color, in_stock])
			eng_info.text = "  Engines: %s" % ", ".join(parts)
			dialog_vbox.add_child(eng_info)

	add_child(dialog)
	dialog.popup_centered()

func _on_prod_auto_build_engines(rocket_index: int, dialog: AcceptDialog):
	dialog.queue_free()
	var result = game_manager.auto_order_engines_for_rocket(rocket_index)
	if result > 0:
		_show_toast("Queued %d engine(s) — assemble rocket when they're ready" % result)
		_update_production_ui()
	elif result == 0:
		_show_toast("No engines needed")
	else:
		_show_toast("Cannot order engines: %s" % game_manager.get_last_order_error())

func _on_prod_rocket_selected(rocket_index: int, dialog: AcceptDialog):
	dialog.queue_free()
	# Cut a revision and start the order
	var rev = game_manager.cut_rocket_revision(rocket_index, "mfg")
	if rev < 0:
		_show_toast("Failed to cut rocket revision")
		return
	var result = game_manager.start_rocket_order(rocket_index, rev)
	if result >= 0:
		_show_toast("Rocket assembly started!")
		_update_production_ui()
	else:
		_show_toast("Cannot start rocket: %s" % game_manager.get_last_order_error())

func _on_prod_auto_assign_teams():
	var assigned = game_manager.auto_assign_manufacturing_teams()
	if assigned > 0:
		_show_toast("Auto-assigned %d team%s" % [assigned, "s" if assigned != 1 else ""])
		_update_production_ui()
	else:
		_show_toast("No idle manufacturing teams to assign")

func _on_manufacturing_changed():
	if current_tab == Tab.PRODUCTION:
		_update_production_ui()

func _on_inventory_changed():
	if current_tab == Tab.PRODUCTION:
		_update_production_ui()

func _format_money_short(value: float) -> String:
	if value >= 1000:
		return "%.0f" % value
	elif value >= 100:
		return "%.0f" % value
	elif value >= 10:
		return "%.1f" % value
	else:
		return "%.2f" % value

func _update_status_bar():
	fame_label.text = game_manager.get_fame_formatted()
	money_label.text = game_manager.get_money_formatted()
	date_label.text = game_manager.get_date_formatted()

func _update_pause_indicator():
	var is_paused = game_manager.is_time_paused()
	if pause_indicator:
		pause_indicator.visible = is_paused
	_last_paused_state = is_paused

func _show_tab(tab: Tab):
	# Hide all content areas
	for key in content_areas:
		var area = content_areas[key]
		if area == null:
			push_error("Content area for tab %s is null!" % key)
			continue
		area.visible = false

	# Show selected content area
	var selected = content_areas[tab]
	if selected == null:
		push_error("Selected content area for tab %s is null!" % tab)
		return
	selected.visible = true
	current_tab = tab

	# Update tab button states
	for t in tab_buttons:
		tab_buttons[t].button_pressed = (t == tab)

# Signal handlers for status bar updates
func _on_money_changed(_new_amount: float):
	money_label.text = game_manager.get_money_formatted()
	if current_tab == Tab.FINANCE:
		_update_finance_ui()

func _on_date_changed(_new_day: int):
	date_label.text = game_manager.get_date_formatted()
	# Note: Don't rebuild research UI on every date tick - it destroys buttons too fast
	# Updates happen via work_event signals instead

func _on_fame_changed(_new_fame: float):
	fame_label.text = game_manager.get_fame_formatted()

func _on_time_paused():
	_update_pause_indicator()

func _on_time_resumed():
	_update_pause_indicator()

func _on_work_event(event_type: String, data: Dictionary):
	# Show toast notifications for important events
	var message = ""
	var refresh_research = false
	var refresh_production = false
	var refresh_finance = false

	match event_type:
		"design_phase_complete":
			var phase = data.get("phase_name", "")
			message = "Design phase complete: %s" % phase
			refresh_research = true
		"design_flaw_discovered":
			var flaw_name = data.get("flaw_name", "unknown")
			message = "Flaw discovered: %s" % flaw_name
			refresh_research = true
		"design_flaw_fixed":
			var flaw_name = data.get("flaw_name", "unknown")
			message = "Flaw fixed: %s" % flaw_name
			refresh_research = true
		"engine_flaw_discovered":
			var flaw_name = data.get("flaw_name", "unknown")
			message = "Engine flaw found: %s" % flaw_name
			refresh_research = true
		"engine_flaw_fixed":
			var flaw_name = data.get("flaw_name", "unknown")
			message = "Engine flaw fixed: %s" % flaw_name
			refresh_research = true
		"engine_manufactured":
			var name = data.get("display_name", "Engine")
			message = "Engine manufactured: %s" % name
			refresh_production = true
			refresh_research = true
		"rocket_assembled":
			var name = data.get("display_name", "Rocket")
			message = "Rocket assembled: %s" % name
			refresh_production = true
			refresh_research = true
		"manufacturing_order_complete":
			var name = data.get("display_name", "Order")
			message = "Production complete: %s" % name
			refresh_production = true
			refresh_research = true
		"team_ramped_up":
			var team_id = data.get("team_id", 0)
			message = "Team %d ready to work" % team_id
			refresh_research = true
		"salary_deducted":
			var amount = data.get("amount", 0)
			message = "Monthly salaries: $%.0fK" % (amount / 1000.0)
			refresh_finance = true
		"floor_space_completed":
			var units = data.get("units", 0)
			message = "Floor space completed: %d units" % units
			refresh_production = true
		_:
			return  # Don't show toast for unknown events

	if refresh_research:
		_update_research_ui()
	if refresh_production:
		_update_production_ui()
	if refresh_finance and current_tab == Tab.FINANCE:
		_update_finance_ui()

	if message != "":
		_show_toast(message)

func _show_toast(message: String):
	# Create a simple toast notification
	var toast = Label.new()
	toast.text = message
	toast.add_theme_font_size_override("font_size", 14)
	toast.add_theme_color_override("font_color", Color(1, 1, 1))
	toast.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	toast.modulate = Color(1, 1, 1, 0)  # Start invisible

	# Position at top center, stacked below existing toasts
	toast.set_anchors_preset(Control.PRESET_TOP_WIDE)
	toast.position.y = 60 + _active_toasts.size() * 35

	_active_toasts.append(toast)
	add_child(toast)

	# Animate in and out
	var tween = create_tween()
	tween.tween_property(toast, "modulate", Color(1, 1, 1, 1), 0.3)
	tween.tween_interval(2.0)
	tween.tween_property(toast, "modulate", Color(1, 1, 1, 0), 0.5)
	tween.tween_callback(_remove_toast.bind(toast))

func _remove_toast(toast: Label):
	_active_toasts.erase(toast)
	toast.queue_free()
	# Reposition remaining toasts
	for i in range(_active_toasts.size()):
		_active_toasts[i].position.y = 60 + i * 35

func _on_date_label_gui_input(event: InputEvent):
	# Click on date label to toggle pause
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		game_manager.toggle_time_pause()

# Tab button handlers
func _on_map_tab_pressed():
	_show_tab(Tab.MAP)

func _on_missions_tab_pressed():
	_show_tab(Tab.MISSIONS)
	content_areas[Tab.MISSIONS]._update_ui()

func _on_design_tab_pressed():
	_show_tab(Tab.DESIGN)
	var design = content_areas[Tab.DESIGN]
	design.show_select_view()

func _on_launch_site_tab_pressed():
	# Sync design data to ensure latest flaw discoveries are visible
	var design = content_areas[Tab.DESIGN]
	var launch_site = content_areas[Tab.LAUNCH_SITE]
	var designer = design.get_designer()
	if designer:
		game_manager.refresh_current_design()
		game_manager.sync_design_to(designer)
		launch_site.set_designer(designer)
	_show_tab(Tab.LAUNCH_SITE)

func _on_research_tab_pressed():
	_show_tab(Tab.RESEARCH)
	_update_research_ui()  # Refresh when tab is shown

func _on_finance_tab_pressed():
	_show_tab(Tab.FINANCE)
	_update_finance_ui()

func _on_production_tab_pressed():
	_show_tab(Tab.PRODUCTION)
	_update_production_ui()

# Public API for other scripts to access game manager
func get_game_manager() -> GameManager:
	return game_manager

# Switch to a specific tab programmatically
func switch_to_tab(tab: Tab):
	_show_tab(tab)

# Missions content signal handlers
func _on_contract_selected(_contract_id: int):
	# When a contract is selected, switch to design tab and show design selection
	var design = content_areas[Tab.DESIGN]
	design.show_select_view()
	_show_tab(Tab.DESIGN)

func _on_design_requested():
	# When design is requested from active contract, switch to design tab
	_show_tab(Tab.DESIGN)

func _on_design_back_requested():
	_show_tab(Tab.MISSIONS)

# Design content signal handlers
func _on_testing_requested():
	# When testing is requested, switch to launch site tab
	# Pass the designer to launch site content
	var design = content_areas[Tab.DESIGN]
	var launch_site = content_areas[Tab.LAUNCH_SITE]
	var designer = design.get_designer()
	if designer:
		# Refresh design from saved designs to get latest flaw discoveries
		game_manager.refresh_current_design()
		game_manager.sync_design_to(designer)
		launch_site.set_designer(designer)
	_show_tab(Tab.LAUNCH_SITE)

# Launch site content signal handlers
func _on_launch_requested():
	# Get the designer from design content
	var design = content_areas[Tab.DESIGN]
	var designer = design.get_designer()

	# Ensure design is saved before launch
	if designer:
		game_manager.sync_design_from(designer)
		game_manager.ensure_design_saved(designer)

	# Consume a manufactured rocket from inventory
	if not game_manager.consume_rocket_for_current_design():
		_show_toast("No manufactured rocket available for launch")
		return

	# Show launch overlay
	launch_overlay.show_launch(game_manager, designer)

# Launch overlay signal handlers
func _on_launch_completed(success: bool):
	if success:
		# Success - go back to missions to select new contract
		_show_tab(Tab.MISSIONS)
	else:
		# Failure - stay on launch site for testing/fixing
		_show_tab(Tab.LAUNCH_SITE)
		# Explicitly refresh the testing view to show newly discovered flaws
		# (visibility notification won't fire since tab was already visible under overlay)
		var launch_site = content_areas[Tab.LAUNCH_SITE]
		launch_site.refresh_testing_view()
