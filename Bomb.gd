extends StaticBody2D

onready var boomPacked = preload("res://Boom.tscn")
onready var boomScript = preload("res://Boom.gd")

var duration = 3
var radius = 2
onready var anim = get_node("AnimBomb")
var inArea = []
var exploding = false
var max_tiles = 20 # The ammount of tiles each ray will collide with.
onready var rays = $Raycasts # The rays parent node.

var from_player = null

# Called when the node enters the scene tree for the first time.
func _ready():
	anim.play("Bomb")
	var timer = Timer.new()
	timer.connect("timeout", self, "explode")
	add_child(timer)
	timer.start(duration)

func explode():
	if exploding:
		return
	exploding = true
	if not self.from_player == null:
		self.from_player.bombs += 1
	
	var colliders = [] # The colliding objects go here. This is a temporary array.
	var final_colliders = [] # The final colliding objects (without duplicates) go here

	for ray in rays.get_children(): # Loop through all the rays.
		ray.add_exception(self)
		while ray.is_colliding():
			var collider = ray.get_collider()
			colliders.append(collider)
			ray.add_exception(collider)
			ray.force_raycast_update()
			if collider != self:
				break

	# Remove duplicates.
	# We have to remove duplicates because all the rays will collide with the first tile.
	for collider in colliders: # Loop through all the colliders.
		if not collider in final_colliders: # If the collider is not in the "final_colliders" array...
			final_colliders.append(collider) # ... add it.

	# Add an explosion to each collider.
	var tiles = []
	var root = get_tree().get_root()
	var tileMap = root.get_node("Main").get_node("Map")
	for collider in final_colliders: # Loop through all the colliders.
		if collider.is_in_group("Box"):
			tiles.append(tileMap.world_to_map(collider.position) + Vector2(0, -1))
		if collider.is_in_group("Destroyable"):
			collider.explode()
	
	var currentPos = tileMap.world_to_map(self.position)
	for d in range(0, 4):
		for r in range(0, radius+1):
			var newPos = currentPos
			if d == 0:
				newPos += Vector2(r, 0)
			elif d == 1:
				newPos -= Vector2(r, 0)
			elif d == 2:
				newPos += Vector2(0, r)
			elif d == 3:
				newPos -= Vector2(0, r)
			if newPos in tiles:
				break
			# TODO clean
			if newPos.x < 2 or newPos.x > 13 \
			or newPos.y < 2 or newPos.y > 13:
				break
			# Instantiate explosion
			var boom = boomPacked.instance()
			boom.set_script(boomScript)
			boom.z_index = 3
			# TODO clean this values
			boom.position = (newPos * 120) + Vector2(60, 60)
			root.add_child(boom)

	queue_free()


func _on_bomb_body_enter(object):
	if not object in inArea:
		print("New for ")
		print(self)
		inArea.append(object)

func _on_bomb_body_exit(object):
	inArea.erase(object)