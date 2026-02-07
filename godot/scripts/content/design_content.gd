extends Control

## Design content with two sub-views:
## - Design select view: List of saved designs
## - Design editor view: Rocket designer

signal testing_requested
signal launch_requested

enum View { SELECT, EDITOR }

var current_view: View = View.SELECT
var game_manager: GameManager = null

@onready var select_view = $DesignSelectView
@onready var editor_view = $DesignEditorView

func _ready():
	# Connect design select signals
	select_view.design_selected.connect(_on_design_selected)
	select_view.back_requested.connect(_on_select_back_requested)

	# Connect design editor signals
	editor_view.back_requested.connect(_on_editor_back_requested)
	editor_view.testing_requested.connect(_on_editor_testing_requested)
	editor_view.launch_requested.connect(_on_editor_launch_requested)
	editor_view.submit_to_engineering_requested.connect(_on_editor_submit_to_engineering)

func set_game_manager(gm: GameManager):
	game_manager = gm
	select_view.set_game_manager(gm)
	editor_view.set_game_manager(gm)

func show_select_view():
	select_view.visible = true
	editor_view.visible = false
	current_view = View.SELECT
	# Refresh the list when showing
	if game_manager:
		select_view._update_ui()

func show_editor_view():
	select_view.visible = false
	editor_view.visible = true
	current_view = View.EDITOR

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

func _on_select_back_requested():
	# When back is pressed in select view, we stay on the tab
	# but could emit a signal if needed
	pass

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

# Called when this content becomes visible
func _notification(what):
	if what == NOTIFICATION_VISIBILITY_CHANGED and visible:
		# Refresh the select view when becoming visible
		if current_view == View.SELECT and game_manager:
			select_view._update_ui()
