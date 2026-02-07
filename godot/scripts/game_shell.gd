extends Control

## Main game shell with tabbed interface
## Handles status bar updates and tab switching

enum Tab { MAP, MISSIONS, DESIGN, LAUNCH_SITE, RESEARCH, FINANCE, PRODUCTION }

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

func _ready():
	# Connect GameManager signals
	game_manager.money_changed.connect(_on_money_changed)
	game_manager.date_changed.connect(_on_date_changed)
	game_manager.fame_changed.connect(_on_fame_changed)
	game_manager.time_paused.connect(_on_time_paused)
	game_manager.time_resumed.connect(_on_time_resumed)
	game_manager.work_event_occurred.connect(_on_work_event)

	# Initialize content areas with game manager
	_setup_missions_content()
	_setup_design_content()
	_setup_launch_site_content()
	_setup_launch_overlay()
	_setup_research_content()

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
	teams_title.text = "Rocket Engineers"
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
	hire_btn.text = "+ Hire Team"
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

	# Update labels
	var count = game_manager.get_team_count()
	if _research_team_count_label:
		_research_team_count_label.text = "Teams: %d" % count
	if _research_salary_label:
		_research_salary_label.text = "Monthly salary: %s" % game_manager.get_total_monthly_salary_formatted()

	# Clear and rebuild team cards
	for child in _research_teams_container.get_children():
		child.queue_free()

	var team_ids = game_manager.get_all_team_ids()
	for id in team_ids:
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
		else:
			status_label.text = "Assigned"
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

	var teams_label = Label.new()
	if teams_count > 0:
		teams_label.text = "%d team%s assigned" % [teams_count, "s" if teams_count > 1 else ""]
		teams_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
	else:
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
		unassign_btn.text = "Unassign All"
		unassign_btn.add_theme_font_size_override("font_size", 12)
		unassign_btn.pressed.connect(_on_unassign_teams_pressed.bind(index))
		btn_hbox.add_child(unassign_btn)

	return panel

func _on_design_card_toggle(index: int):
	_expanded_design_cards[index] = not _expanded_design_cards.get(index, false)
	_update_research_designs()

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

	# Teams info (only if not Untested)
	if base_status != "Untested":
		var teams_label = Label.new()
		if teams_count > 0:
			teams_label.text = "%d team%s assigned" % [teams_count, "s" if teams_count > 1 else ""]
			teams_label.add_theme_color_override("font_color", Color(0.7, 0.7, 0.7))
		else:
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
			unassign_btn.text = "Unassign All"
			unassign_btn.add_theme_font_size_override("font_size", 12)
			unassign_btn.pressed.connect(_on_unassign_engine_teams_pressed.bind(index))
			btn_hbox.add_child(unassign_btn)

	return panel

func _on_engine_card_toggle(index: int):
	_expanded_engine_cards[index] = not _expanded_engine_cards.get(index, false)
	_update_research_engines()

func _on_submit_engine_pressed(index: int):
	game_manager.submit_engine_to_refining(index)
	_update_research_ui()

func _on_assign_engine_team_pressed(engine_index: int):
	# Find an available (unassigned, not ramping) team
	var team_ids = game_manager.get_all_team_ids()
	for id in team_ids:
		var is_assigned = game_manager.is_team_assigned(id)
		var is_ramping = game_manager.is_team_ramping_up(id)
		if not is_assigned and not is_ramping:
			game_manager.assign_team_to_engine(id, engine_index)
			_update_research_ui()
			return

	# If no available team, try any unassigned team (even if ramping)
	for id in team_ids:
		if not game_manager.is_team_assigned(id):
			game_manager.assign_team_to_engine(id, engine_index)
			_update_research_ui()
			return

	# No teams available - show a message
	_show_toast("No available teams. Hire more teams!")

func _on_unassign_engine_teams_pressed(engine_index: int):
	# Unassign all teams from this engine
	var team_ids = game_manager.get_all_team_ids()
	for id in team_ids:
		var assignment = game_manager.get_team_assignment(id)
		if assignment.get("type") == "engine" and assignment.get("engine_design_id") == engine_index:
			game_manager.unassign_team(id)
	_update_research_ui()

func _on_research_hire_pressed():
	game_manager.hire_team()

func _on_research_teams_changed():
	_update_research_teams()

func _on_research_designs_changed():
	_update_research_designs()

func _on_research_update_timer():
	# Periodic update of research UI (for progress bars and counters)
	if current_tab == Tab.RESEARCH:
		_update_research_ui()

func _on_assign_team_pressed(design_index: int):
	# Find an available (unassigned, not ramping) team
	var team_ids = game_manager.get_all_team_ids()
	for id in team_ids:
		var is_assigned = game_manager.is_team_assigned(id)
		var is_ramping = game_manager.is_team_ramping_up(id)
		if not is_assigned and not is_ramping:
			game_manager.assign_team_to_design(id, design_index)
			_update_research_ui()
			return

	# If no available team, try any unassigned team (even if ramping)
	for id in team_ids:
		if not game_manager.is_team_assigned(id):
			game_manager.assign_team_to_design(id, design_index)
			_update_research_ui()
			return

	# No teams available - show a message
	_show_toast("No available teams. Hire more teams!")

func _on_unassign_teams_pressed(design_index: int):
	# Unassign all teams from this design
	var team_ids = game_manager.get_all_team_ids()
	for id in team_ids:
		var assignment = game_manager.get_team_assignment(id)
		if assignment.get("type") == "design" and assignment.get("design_index") == design_index:
			game_manager.unassign_team(id)
	_update_research_ui()

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
		"team_ramped_up":
			var team_id = data.get("team_id", 0)
			message = "Team %d ready to work" % team_id
			refresh_research = true
		"salary_deducted":
			var amount = data.get("amount", 0)
			message = "Monthly salaries: $%.0fK" % (amount / 1000.0)
		_:
			return  # Don't show toast for unknown events

	if refresh_research:
		_update_research_ui()

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

	# Position at top center
	toast.set_anchors_preset(Control.PRESET_TOP_WIDE)
	toast.position.y = 60

	add_child(toast)

	# Animate in and out
	var tween = create_tween()
	tween.tween_property(toast, "modulate", Color(1, 1, 1, 1), 0.3)
	tween.tween_interval(2.0)
	tween.tween_property(toast, "modulate", Color(1, 1, 1, 0), 0.5)
	tween.tween_callback(toast.queue_free)

func _on_date_label_gui_input(event: InputEvent):
	# Click on date label to toggle pause
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		game_manager.toggle_time_pause()

# Tab button handlers
func _on_map_tab_pressed():
	_show_tab(Tab.MAP)

func _on_missions_tab_pressed():
	_show_tab(Tab.MISSIONS)

func _on_design_tab_pressed():
	_show_tab(Tab.DESIGN)

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

func _on_production_tab_pressed():
	_show_tab(Tab.PRODUCTION)

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
