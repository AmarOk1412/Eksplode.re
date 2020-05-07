extends StaticBody2D

# Declare member variables here. Examples:
# var a = 2
# var b = "text"
onready var anim = get_node("AnimBomb")

# Called when the node enters the scene tree for the first time.
func _ready():
	anim.play("Bomb")

# Called every frame. 'delta' is the elapsed time since the previous frame.
#func _process(delta):
#	pass
