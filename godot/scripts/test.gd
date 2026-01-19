extends Node

func _ready():
	print("GDScript test ready")
	var test_node = $TestNode
	if test_node:
		var result = test_node.test_connection()
		print("Rust test result: ", result)
	else:
		print("ERROR: TestNode not found!")
