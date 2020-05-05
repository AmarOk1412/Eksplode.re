extends KinematicBody2D
onready var bombPacked = preload("res://Bomb.tscn")
onready var anim = get_node("AnimationPlayer")

# Player movement speed
export var speed = 300

enum Direction {
	Up,
	Down,
	Left,
	Right
}
var lastDir = Direction.Down

func _physics_process(delta):
	# Get player input
	var direction: Vector2
	direction.x = Input.get_action_strength("ui_right") - Input.get_action_strength("ui_left")
	direction.y = Input.get_action_strength("ui_down") - Input.get_action_strength("ui_up")

	anim.flip_h = Input.is_action_pressed("ui_left") || lastDir == Direction.Left
	
	if Input.is_action_pressed("ui_down"):
		lastDir = Direction.Down
		anim.play("Down")
	elif Input.is_action_pressed("ui_up"):
		lastDir = Direction.Up
		anim.play("Up")
	elif Input.is_action_pressed("ui_right"):
		lastDir = Direction.Right
		anim.play("Right")
	elif Input.is_action_pressed("ui_left"):
		lastDir = Direction.Left
		anim.play("Right")
	elif lastDir == Direction.Down:
		anim.play("Base")
	elif lastDir == Direction.Up:
		anim.play("BaseRevert")
	else:
		anim.play("Side")
	# If input is digital, normalize it for diagonal movement
	if abs(direction.x) == 1 and abs(direction.y) == 1:
		direction = direction.normalized()
	
	# Apply movement
	var movement = speed * direction * delta
	move_and_collide(movement)

func _input(ev):
	if ev is InputEventKey and ev.scancode == KEY_SPACE and not ev.echo:
		var bomb = bombPacked.instance()
		bomb.position = self.position
		get_tree().get_root().add_child(bomb)

func _ready():
	anim.play("Base")