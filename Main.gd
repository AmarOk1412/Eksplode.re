extends Node

onready var destructibleBox = preload("res://DestructibleBox.tscn")
onready var boxScript = preload("res://DestructibleBox.gd")
var boxTexture = preload("res://Sprites/Box/box.png")
const CELL_SIZE = 120
const START_X = 2
const START_Y = 2
const END_X = 13
const END_Y = 13


# Todo clean
func _ready():
	for x in range(START_X, END_X):
		for y in range(START_Y, END_Y):
			if x%2 == 1 and y % 2 == 1:
				var box = destructibleBox.instance()
				box.z_index = 0
				box.get_node("Sprite").set_texture(boxTexture)
				box.add_to_group("Box")
				box.position = Vector2(x*CELL_SIZE, y*CELL_SIZE)+ Vector2(CELL_SIZE/2, CELL_SIZE)
				var root = get_tree().get_root()
				self.add_child(box)
			elif (abs(x-START_X)<=1 or abs(x-(END_X-1))<=1) \
				and (abs(y-START_Y)<=1 or abs(y-(END_Y-1))<=1):
				continue
			elif randi()%3+1 != 2:
				var box = destructibleBox.instance()
				box.set_script(boxScript)
				box.z_index = 0
				box.add_to_group("Destroyable")
				box.add_to_group("Box")
				box.position = Vector2(x*CELL_SIZE, y*CELL_SIZE)+ Vector2(CELL_SIZE/2, CELL_SIZE)
				var root = get_tree().get_root()
				self.add_child(box)