extends KinematicBody2D

onready var boomPacked = preload("res://Boom.tscn")
onready var boomScript = preload("res://Boom.gd")

var duration = 3
var radius = 2
onready var anim = get_node("AnimBomb")
var exploding = false
var max_tiles = 20 # The ammount of tiles each ray will collide with.
onready var rays = $Raycasts # The rays parent node.

var from_player = null
var moveVector = Vector2()


# Called when the node enters the scene tree for the first time.
func _ready():
	anim.play("Bomb")
	var timer = Timer.new()
	timer.connect("timeout", self, "explode")
	add_child(timer)
	timer.start(duration)


func _physics_process(delta):
	if Vector2() == moveVector:
		return
	var root = get_tree().get_root()
	var tileMap = root.get_node("Main").get_node("Map")
	var nextTileEmpty = true
	var colliders = [] # The colliding objects go here. This is a temporary array.
	var ray = $Raycasts/West
	if moveVector.x > 0:
		ray = $Raycasts/East
	elif moveVector.y > 0:
		ray = $Raycasts/South
	elif moveVector.y < 0:
		ray = $Raycasts/North
	# Get next collider
	ray.add_exception(self)
	while ray.is_colliding():
		var collider = ray.get_collider()
		if not collider.is_in_group("Player"): #TODOs
			colliders.append(collider)
		ray.add_exception(collider)
		ray.force_raycast_update()
		if collider != self:
			break
	var nextTile = tileMap.world_to_map(self.position) + moveVector
	if nextTile.x > 12 or nextTile.x < 2 or nextTile.y > 12 or nextTile.y < 2:
		 nextTileEmpty = false
	for collider in colliders:
		# TODO avoid this
		var vectorOffset = Vector2()
		if collider.is_in_group("Box"):
			vectorOffset.y = 1
		if tileMap.world_to_map(collider.position) == nextTile + vectorOffset:
			nextTileEmpty = false
		ray.remove_exception(collider)
	ray.remove_exception(self)
	
	var stopMove = false
	# TODO CLEAN THIS
	if not nextTileEmpty:
		if moveVector.x < 0:
			stopMove = position.x < (nextTile.x+1)*120+60
		elif moveVector.x > 0:
			stopMove = position.x >= nextTile.x*120-60
		elif moveVector.y > 0:
			stopMove = position.y >= nextTile.y*120-60
		elif moveVector.y < 0:
			stopMove = position.y < (nextTile.y+1)*120+60
	if not stopMove:
		if moveVector.x < 0:
			position.x = lerp(position.x, position.x-800, delta)
		elif moveVector.x > 0:
			position.x = lerp(position.x, position.x+800, delta)
		elif moveVector.y > 0:
			position.y = lerp(position.y, position.y+800, delta)
		elif moveVector.y < 0:
			position.y = lerp(position.y, position.y-800, delta)
	else:
		moveVector = Vector2()


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
		print(collider.get_groups())
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
			if newPos.x < 2 or newPos.x > 12 \
			or newPos.y < 2 or newPos.y > 12:
				break
			# Instantiate explosion
			var boom = boomPacked.instance()
			boom.set_script(boomScript)
			boom.z_index = 3
			# TODO clean this values
			boom.position = (newPos * 120) + Vector2(60, 60)
			root.add_child(boom)

	queue_free()

func setRadius(r):
	self.radius = r
	$Raycasts/East.cast_to = Vector2(120 * radius, 0)
	$Raycasts/West.cast_to = Vector2(120 * -radius, 0)
	$Raycasts/South.cast_to = Vector2(0, 120 * radius)
	$Raycasts/North.cast_to = Vector2(0, 120 * -radius)

func _on_Area2D_body_entered(body):
	if body.is_in_group("Player"):
		print("NEAR")
		body.near(self)
