extends Node

onready var destructibleBox = preload("res://DestructibleBox.tscn")
onready var boxScript = preload("res://DestructibleBox.gd")
const CELL_SIZE = 120
var START_X = 2
var START_Y = 2
var END_X = 13
var END_Y = 13

# Declare member variables here. Examples:
# var a = 2
# var b = "text"

# Called when the node enters the scene tree for the first time.
func _ready():
	for x in range(START_X, END_X):
		for y in range(START_Y, END_Y):
			if (abs(x-START_X)<=1 or abs(x-(END_X-1))<=1) \
				and (abs(y-START_Y)<=1 or abs(y-(END_Y-1))<=1):
				continue
			if x%2 == 1 and y % 2 == 1:
				continue
			if randi()%3+1 == 2:
				continue
			var box = destructibleBox.instance()
			box.set_script(boxScript)
			box.z_index = 0
			box.add_to_group("Destroyable")
			box.add_to_group("Box")
			box.position = Vector2(x*CELL_SIZE, y*CELL_SIZE)+ Vector2(CELL_SIZE/2, CELL_SIZE)
			var root = get_tree().get_root()
			self.add_child(box)

# Called every frame. 'delta' is the elapsed time since the previous frame.
#func _process(delta):
#	pass