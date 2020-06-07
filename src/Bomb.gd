# BSD 3-Clause License
#
# Copyright (c) 2020, Sébastien Blin <sebastien.blin@enconn.fr>
# All rights reserved.
#
# Redistribution and use in source and binary forms, with or without
# modification, are permitted provided that the following conditions are met:
#
# * Redistributions of source code must retain the above copyright notice, this
#   list of conditions and the following disclaimer.
#
# * Redistributions in binary form must reproduce the above copyright notice,
#   this list of conditions and the following disclaimer in the documentation
#   and/or other materials provided with the distribution.
#
# * Neither the name of the copyright holder nor the names of its
#   contributors may be used to endorse or promote products derived from
#   this software without specific prior written permission.
#
# THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
# AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
# IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
# DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
# FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
# DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
# SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
# CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
# OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
# OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

extends KinematicBody2D

onready var boomPacked = preload("res://src/Boom.tscn")
onready var boomScript = preload("res://src/Boom.gd")

const prefs = preload("res://src/Utils/constant.gd")

var duration = 3
var radius = 2
onready var anim = get_node("AnimBomb")
var exploding = false
onready var rays = $Raycasts # The rays parent node.

var from_player = null
var moveVector = Vector2()
var timer = Timer.new()

func _ready():
	anim.play("Bomb")
	self.timer.connect("timeout", self, "explode")
	add_child(self.timer)
	self.timer.start(duration)

const COMPLETION_TIME = 0.3
var time_passed = 1.0

var START = Vector2(300, 300)
var END = Vector2(540, 300)
var MID = Vector2(420, 160)
var movingVec = Vector2()

func push(offsetVec):
	self.time_passed = 0.0
	self.START = self.position
	self.END = self.START + offsetVec
	self.MID = self.START + offsetVec/2 + Vector2(0, -120)
	self.timer.paused = true
	self.movingVec = offsetVec


func _physics_process(delta):

	if time_passed < COMPLETION_TIME:
		time_passed += delta
		var f = time_passed / COMPLETION_TIME
		var Y = START.linear_interpolate(MID, f)
		var Z = MID.linear_interpolate(END, f)
		self.position = Y.linear_interpolate(Z, f)
	elif self.movingVec != Vector2():
		# Check if we are on a Box, a Player or Bomb
		var root = get_tree().get_root()
		var tileMap = root.get_node("Game").get_node("Map")
		var tilePos = tileMap.world_to_map(self.position)
		for bomb in get_tree().get_nodes_in_group("Bomb"):
			var bombPos = tileMap.world_to_map(bomb.get_position())
			if tilePos == bombPos and bomb.movingVec == Vector2():
				self.push(self.movingVec)
				return
		for player in get_tree().get_nodes_in_group("Player"):
			var playerPos = tileMap.world_to_map(player.get_position())
			if tilePos == playerPos:
				self.push(self.movingVec)
				return
		for box in get_tree().get_nodes_in_group("Box"):
			var boxPos = tileMap.world_to_map(box.get_position()) + Vector2(0, -1)
			if tilePos == boxPos:
				self.push(self.movingVec)
				return
		if tilePos.x >= prefs.END_X or tilePos.x < prefs.START_X or tilePos.y >= prefs.END_Y or tilePos.y < prefs.START_Y:
			if tilePos.x >= prefs.END_X + 6:
				self.position -= Vector2(prefs.CELL_SIZE, 0) * (prefs.END_X-prefs.START_X+8)
			elif tilePos.x <= prefs.START_X - 6:
				self.position += Vector2(prefs.CELL_SIZE, 0) * (prefs.END_X-prefs.START_X+8)
			elif tilePos.y >= prefs.END_Y + 6:
				self.position -= Vector2(0, prefs.CELL_SIZE) * (prefs.END_Y-prefs.START_Y+8)
			elif tilePos.y <= prefs.START_Y - 6:
				self.position += Vector2(0, prefs.CELL_SIZE) * (prefs.END_Y-prefs.START_Y+8)
			self.push(self.movingVec)
			return
		self.movingVec = Vector2()
		self.timer.paused = false

	if Vector2() == moveVector:
		return
	var root = get_tree().get_root()
	var tileMap = root.get_node("Game").get_node("Map")
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
		if not collider.is_in_group("Player"):
			colliders.append(collider)
		ray.add_exception(collider)
		ray.force_raycast_update()
		if collider != self:
			break
	var nextTile = tileMap.world_to_map(self.position) + moveVector
	if nextTile.x >= prefs.END_X or nextTile.x < prefs.START_X or nextTile.y >= prefs.END_Y or nextTile.y < prefs.START_Y:
		 nextTileEmpty = false
	for collider in colliders:
		ray.remove_exception(collider)
		if collider.is_in_group("Item"): # Just erase that item
			continue
		var vectorOffset = Vector2()
		if collider.is_in_group("Box"):
			vectorOffset.y = 1
		if tileMap.world_to_map(collider.position) == nextTile + vectorOffset:
			nextTileEmpty = false
	ray.remove_exception(self)

	var stopMove = false
	if not nextTileEmpty:
		if moveVector.x < 0:
			stopMove = position.x < (nextTile.x+1)*prefs.CELL_SIZE+prefs.CELL_SIZE/2
		elif moveVector.x > 0:
			stopMove = position.x >= nextTile.x*prefs.CELL_SIZE-prefs.CELL_SIZE/2
		elif moveVector.y > 0:
			stopMove = position.y >= nextTile.y*prefs.CELL_SIZE-prefs.CELL_SIZE/2
		elif moveVector.y < 0:
			stopMove = position.y < (nextTile.y+1)*prefs.CELL_SIZE+prefs.CELL_SIZE/2
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
	var tileMap = root.get_node("Game").get_node("Map")
	for collider in final_colliders: # Loop through all the colliders.
		if collider.is_in_group("Box"):
			tiles.append(tileMap.world_to_map(collider.position) + Vector2(0, -1))
		if is_network_master():
			# Handle explosions server side
			if collider.is_in_group("Player"):
				gamestate.master_explode_player(collider.get_network_master())
			elif collider.is_in_group("Box") && collider.is_in_group("Destroyable"):
				gamestate.master_explode_box(collider.position)
			elif collider.is_in_group("Item"):
				gamestate.master_explode_item(collider.position)

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
			if newPos.x < 2 or newPos.x > 12 \
			or newPos.y < 2 or newPos.y > 12:
				break
			# Instantiate explosion
			var boom = boomPacked.instance()
			boom.set_script(boomScript)
			boom.z_index = 3
			boom.position = (newPos * prefs.CELL_SIZE) + Vector2(prefs.CELL_SIZE/2, prefs.CELL_SIZE/2)
			root.add_child(boom)
	var bombTrack = root.get_node("Game").get_node("BombTrack")
	bombTrack.play()
	queue_free()

func setRadius(r):
	self.radius = r
	$Raycasts/East.cast_to = Vector2(prefs.CELL_SIZE * radius, 0)
	$Raycasts/West.cast_to = Vector2(prefs.CELL_SIZE * -radius, 0)
	$Raycasts/South.cast_to = Vector2(0, prefs.CELL_SIZE * radius)
	$Raycasts/North.cast_to = Vector2(0, prefs.CELL_SIZE * -radius)

func _on_Area2D_body_entered(body):
	if body.is_in_group("Player"):
		body.near(self)
