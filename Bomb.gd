extends StaticBody2D

var duration = 3
onready var anim = get_node("AnimBomb")
var inArea = []

# Called when the node enters the scene tree for the first time.
func _ready():
	anim.play("Bomb")
	var timer = Timer.new()
	timer.connect("timeout",self,"_on_timer_timeout")
	add_child(timer)
	timer.start(duration)

func _on_timer_timeout():
	for o in inArea:
		if o.is_in_group("Destroyable"):
			o.explode()
	queue_free()


func _on_bomb_body_enter(object):
	if not object in inArea:
		inArea.append(object)

func _on_bomb_body_exit(object):
	inArea.erase(object)