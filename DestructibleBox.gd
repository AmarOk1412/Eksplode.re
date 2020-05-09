extends StaticBody2D

onready var itemPacked = preload("res://Item.tscn")
onready var itemScript = preload("res://Item.gd")

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
		item.position = (tilePos * 120) + Vector2(60, -60)
		root.add_child(item)
		
	queue_free()