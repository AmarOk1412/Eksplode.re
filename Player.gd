extends KinematicBody2D
onready var bombPacked = preload("res://Bomb.tscn")
onready var bombScript = preload("res://Bomb.gd")
onready var anim = get_node("AnimationPlayer")
const prefs = preload("res://Utils/constant.gd")

# Player movement speed
var speed = 300
var bombs = 1
var radius = 2
var repelBombs = false
var type = "Red_"

# Effects
var timerEffect = Timer.new()
enum Effect {
	None,
	Slow,
	Fast,
	Inverted,
	SmallBomb,
	Flu
}
var currentEffect = Effect.None
const EFFECT_DURATION = 15

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
	if currentEffect == Effect.Inverted:
		direction.x = Input.get_action_strength("ui_left") - Input.get_action_strength("ui_right")
		direction.y = Input.get_action_strength("ui_up") - Input.get_action_strength("ui_down")
		anim.flip_h = Input.is_action_pressed("ui_right") || lastDir == Direction.Left
	else:
		direction.x = Input.get_action_strength("ui_right") - Input.get_action_strength("ui_left")
		direction.y = Input.get_action_strength("ui_down") - Input.get_action_strength("ui_up")
		anim.flip_h = Input.is_action_pressed("ui_left") || lastDir == Direction.Left
	
	if currentEffect == Effect.Flu:
		self.drop()
	
	if direction.y > 0:
		lastDir = Direction.Down
		anim.play(type + "Down")
	elif direction.y < 0:
		lastDir = Direction.Up
		anim.play(type + "Up")
	elif direction.x > 0:
		lastDir = Direction.Right
		anim.play(type + "Right")
	elif direction.x < 0:
		lastDir = Direction.Left
		anim.play(type + "Right")
	elif lastDir == Direction.Down:
		anim.play(type + "Base")
	elif lastDir == Direction.Up:
		anim.play(type + "BaseRevert")
	else:
		anim.play(type + "Side")
	# If input is digital, normalize it for diagonal movement
	if abs(direction.x) == 1 and abs(direction.y) == 1:
		direction = direction.normalized()
	
	# Apply movement
	var playerSpeed = speed
	if currentEffect == Effect.Slow:
		playerSpeed = 100
	elif currentEffect == Effect.Fast:
		playerSpeed = speed * 10
	var movement = playerSpeed * direction * delta
	move_and_collide(movement)

func _input(ev):
	if Input.is_action_just_pressed("ui_accept"):
		drop()

func drop():
	if self.bombs <= 0:
		return
	var root = get_tree().get_root()
	var tileMap = root.get_node("Main").get_node("Map")
	var tilePos = tileMap.world_to_map(self.position)
	for bomb in get_tree().get_nodes_in_group("Bomb"):
		var bombPos = tileMap.world_to_map(bomb.position)
		if tilePos == bombPos:
			return
	bombs -= 1
	var bomb = bombPacked.instance()
	bomb.set_script(bombScript)
	bomb.add_to_group("Destroyable")
	bomb.add_to_group("Bomb")
	bomb.z_index = 2
	bomb.from_player = self
	# TODO clean this values
	bomb.position = (tilePos * prefs.CELL_SIZE) + Vector2(prefs.CELL_SIZE/2, prefs.CELL_SIZE/2)
	var finalRadius = self.radius
	if self.currentEffect == Effect.SmallBomb:
		finalRadius = 1
	bomb.setRadius(finalRadius)
	root.add_child(bomb)

func _ready():
	anim.play("Base")

func explode():
	queue_free()

func removeEffect():
	currentEffect = Effect.None

func near(bomb):
	if self.repelBombs:
		if bomb.position.x - prefs.CELL_SIZE/2 > self.position.x:
			bomb.moveVector = Vector2(1,0)
		elif bomb.position.x + prefs.CELL_SIZE/2 < self.position.x:
			bomb.moveVector = Vector2(-1,0)
		elif bomb.position.y - prefs.CELL_SIZE/2 > self.position.y:
			bomb.moveVector = Vector2(0,1)
		elif bomb.position.y + prefs.CELL_SIZE/2 < self.position.y:
			bomb.moveVector = Vector2(0,-1)

func affect():
	timerEffect.stop()
	# Note: random effect and avoid None
	currentEffect = Effect.values()[randi()%(Effect.size() - 1) + 1]
	timerEffect.connect("timeout", self, "removeEffect")
	add_child(timerEffect)
	timerEffect.start(EFFECT_DURATION)