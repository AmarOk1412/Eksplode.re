extends StaticBody2D

const prefs = preload("res://src/Utils/constant.gd")

# Called when the node enters the scene tree for the first time.
func _ready():
	pass # Replace with function body.

func explode():
	if is_network_master() and randi()%4+1 == 2:
		gamestate.new_item(self.position)
		
	queue_free()