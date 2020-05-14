extends StaticBody2D

onready var itemPacked = preload("res://Item.tscn")
onready var itemScript = preload("res://Item.gd")
const prefs = preload("res://Utils/constant.gd")

# Called when the node enters the scene tree for the first time.
func _ready():
	pass # Replace with function body.

func explode():
	if randi()%4+1 == 2:
		var root = get_tree().get_root()
		var tileMap = root.get_node("Main").get_node("Map")
		var tilePos = tileMap.world_to_map(self.position)
		var item = itemPacked.instance()
		item.set_script(itemScript)
		item.add_to_group("Destroyable")
		item.z_index = 2
		# TODO clean this values
		item.position = (tilePos * prefs.CELL_SIZE) + Vector2(prefs.CELL_SIZE/2, -prefs.CELL_SIZE/2)
		root.add_child(item)
		
	queue_free()