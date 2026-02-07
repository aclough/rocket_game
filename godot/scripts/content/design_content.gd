extends Control

## Design content with three sub-views:
## - Design select view: List of saved designs (rockets + engines)
## - Design editor view: Rocket designer
## - Engine editor view: Engine designer

signal testing_requested
signal launch_requested
signal back_requested

enum View { SELECT, EDITOR, ENGINE_EDITOR }

var current_view: View = View.SELECT
var game_manager: GameManager = null

@onready var select_view = $DesignSelectView
@onready var editor_view = $DesignEditorView
@onready var engine_editor_view = $EngineEditorView

func _ready():
	# Connect design select signals
	select_view.design_selected.connect(_on_design_selected)
	select_view.back_requested.connect(_on_select_back_requested)
	select_view.engine_edit_requested.connect(_on_engine_edit_requested)

	# Connect design editor signals
	editor_view.back_requested.connect(_on_editor_back_requested)
	editor_view.testing_requested.connect(_on_editor_testing_requested)
	editor_view.launch_requested.connect(_on_editor_launch_requested)
	editor_view.submit_to_engineering_requested.connect(_on_editor_submit_to_engineering)

	# Connect engine editor signals
	engine_editor_view.back_requested.connect(_on_engine_editor_back_requested)

func set_game_manager(gm: GameManager):
	game_manager = gm
	select_view.set_game_manager(gm)
	editor_view.set_game_manager(gm)
	engine_editor_view.set_game_manager(gm)

func show_select_view():
	select_view.visible = true
	editor_view.visible = false
	engine_editor_view.visible = false
	current_view = View.SELECT
	# Refresh the list when showing
	if game_manager:
		select_view._update_ui()

func show_editor_view():
	select_view.visible = false
	editor_view.visible = true
	engine_editor_view.visible = false
	current_view = View.EDITOR

func show_engine_editor_view(engine_index: int):
	select_view.visible = false
	editor_view.visible = false
	engine_editor_view.visible = true
	engine_editor_view.load_engine(engine_index)
	current_view = View.ENGINE_EDITOR

func get_designer():
	return editor_view.get_designer()

# Design select signals
func _on_design_selected(design_index: int):
	# design_index is -1 for new design, otherwise the saved design index
	# The design_select_screen already loaded the design into game_manager
	# Sync it to the editor
	if game_manager:
		var designer = editor_view.get_designer()
		game_manager.sync_design_to(designer)
	show_editor_view()

func _on_engine_edit_requested(engine_index: int):
	show_engine_editor_view(engine_index)

func _on_select_back_requested():
	back_requested.emit()

# Design editor signals
func _on_editor_back_requested():
	# Sync design back to game manager before leaving
	if game_manager:
		var designer = editor_view.get_designer()
		game_manager.sync_design_from(designer)
	show_select_view()

func _on_editor_testing_requested():
	# Sync design and emit testing signal
	if game_manager:
		var designer = editor_view.get_designer()
		game_manager.sync_design_from(designer)
	testing_requested.emit()

func _on_editor_launch_requested():
	# Same as testing for now
	_on_editor_testing_requested()

func _on_editor_submit_to_engineering():
	# Save the design and submit to engineering
	if game_manager:
		var designer = editor_view.get_designer()
		game_manager.sync_design_from(designer)
		game_manager.ensure_design_saved(designer)
		game_manager.submit_current_to_engineering()
	# Go back to select view to show updated status
	show_select_view()

# Engine editor signals
func _on_engine_editor_back_requested():
	show_select_view()

# Called when this content becomes visible
func _notification(what):
	if what == NOTIFICATION_VISIBILITY_CHANGED and visible:
		# Refresh the select view when becoming visible
		if current_view == View.SELECT and game_manager:
			select_view._update_ui()
