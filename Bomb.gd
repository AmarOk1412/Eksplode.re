extends StaticBody2D

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
	timer.connect("timeout",self,"explode")
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
	for collider in final_colliders: # Loop through all the colliders.
		if collider.is_in_group("Destroyable"):
			collider.explode()
	queue_free()


func _on_bomb_body_enter(object):
	if not object in inArea:
		print("New for ")
		print(self)
		inArea.append(object)

func _on_bomb_body_exit(object):
	inArea.erase(object)