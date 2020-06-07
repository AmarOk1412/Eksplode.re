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

extends Node
const prefs = preload("res://src/Utils/constant.gd")
onready var mainScript = load("res://src/Game.gd")
# Default game port. Can be any number between 1024 and 49151.
const DEFAULT_PORT = 14121

# Max number of players.
const MAX_PEERS = 4

# Names for remote players in id:name format.
var players = {}
var in_lobby = 0
var host_master = false

# Signals
signal player_list_changed()
signal game_error(what)
signal connection_failed()
signal connection_succeeded()

# Timer finish
var timerFinish = Timer.new()
var finishBox = 0
var endAnim = -1
var timerEnd = Timer.new()
var currentWorld = null

# Callback from SceneTree.
func _player_connected(id):
	# Registration of a client beings here, tell the connected player that we are here.
	rpc_id(id, "register_player", players)

# Callback from SceneTree.
func _player_disconnected(id):
	unregister_player(id)

# Callback from SceneTree, only for clients (not server).
func _connected_ok():
	emit_signal("connection_succeeded")

func _connected_fail():
	get_tree().set_network_peer(null) # Remove peer
	emit_signal("connection_failed")

func _server_disconnected():
	emit_signal("game_error", "Server disconnected")

remote func register_player(new_players):
	var id = get_tree().get_rpc_sender_id()
	print("New player connected: " + str(id))
	players[id] = new_players[id]
	self.in_lobby += 1
	emit_signal("player_list_changed")

func unregister_player(id):
	players.erase(id)
	print("Player disconnected: " + str(id))
	emit_signal("player_list_changed")

func reset():
	self.timerFinish = Timer.new()
	self.finishBox = 0
	self.timerEnd = Timer.new()
	self.currentWorld = null
	self.players = {}
	self.in_lobby = 0

func host_game(new_player_name):
	host_master = true
	self.reset()
	var host = NetworkedMultiplayerENet.new()
	host.create_server(DEFAULT_PORT, MAX_PEERS)
	get_tree().set_network_peer(host)
	players[get_tree().get_network_unique_id()] = [new_player_name, ""]
	get_tree().get_root().get_node("Lobby").get_node("RoomLobby").get_node("Start").show()
	self.in_lobby = 1

func join_game(new_player_name, domain, port=DEFAULT_PORT):
	host_master = false
	self.reset()
	var client = NetworkedMultiplayerENet.new()
	client.create_client(domain, port)
	get_tree().set_network_peer(client)
	players[get_tree().get_network_unique_id()] =  [new_player_name, ""]

func begin_game():
	assert(get_tree().is_network_server())
	var player_data = {}
	var idx = 0
	for p in players:
		if idx == 0:
			player_data[p] = [ \
				Vector2(prefs.START_X * prefs.CELL_SIZE, prefs.START_Y * prefs.CELL_SIZE) + Vector2(30, 30),
				players[p][1],
				players[p][0]]
		elif idx == 1:
			player_data[p] = [
			Vector2(prefs.START_X * prefs.CELL_SIZE, prefs.END_Y * prefs.CELL_SIZE) + Vector2(30, -30),
			players[p][1],
			players[p][0]]
		elif idx == 2:
			player_data[p] = [
			Vector2(prefs.END_X * prefs.CELL_SIZE, prefs.START_Y * prefs.CELL_SIZE) + Vector2(-30, 30),
			players[p][1],
			players[p][0]]
		elif idx == 3:
			player_data[p] = [
			Vector2(prefs.END_X * prefs.CELL_SIZE, prefs.END_Y * prefs.CELL_SIZE) + Vector2(-30, -30),
			players[p][1],
			players[p][0]]
		idx += 1

	var boxes = []
	for x in range(prefs.START_X, prefs.END_X):
		for y in range(prefs.START_Y, prefs.END_Y):
			if x%2 == 1 and y % 2 == 1:
				boxes.append([
					false,
					Vector2(x*prefs.CELL_SIZE, y*prefs.CELL_SIZE) + Vector2(prefs.CELL_SIZE/2, prefs.CELL_SIZE)
				])
			elif (abs(x-prefs.START_X)<=1 or abs(x-(prefs.END_X-1))<=1) \
				and (abs(y-prefs.START_Y)<=1 or abs(y-(prefs.END_Y-1))<=1):
				continue
			elif randi()%3+1 != 2:
				boxes.append([
					true,
					Vector2(x*prefs.CELL_SIZE, y*prefs.CELL_SIZE)+ Vector2(prefs.CELL_SIZE/2, prefs.CELL_SIZE)
				])

	var track = randi()%2 + 1
	for p in players:
		rpc_id(p, "pre_start_game", player_data, boxes, track)
	pre_start_game(player_data, boxes, track)

remote func spawn_final_box(boxPos):
	if not self.currentWorld:
		return
	var root = get_tree().get_root()
	var tileMap = root.get_node("Game").get_node("Map")
	var boxTile = tileMap.world_to_map(boxPos)
	for player in get_tree().get_nodes_in_group("Player"):
		var playerPos = tileMap.world_to_map(player.get_position())
		if playerPos == boxTile:
			player.explode()
	for box in get_tree().get_nodes_in_group("Box"):
		var mapBoxPos = tileMap.world_to_map(box.get_position() + Vector2(prefs.CELL_SIZE/2, prefs.CELL_SIZE))
		if mapBoxPos == boxTile + Vector2(1, 2):
			box.queue_free()
	var pos = boxPos + Vector2(prefs.CELL_SIZE/2, prefs.CELL_SIZE)
	self.currentWorld.spawn_box([
			false,
			pos
		])

func spawn_end_box():
	var width = prefs.END_X - prefs.START_X
	if self.currentWorld:
		var boxPos = Vector2()
		if endAnim == 0:
			# From top to bottom
			boxPos = Vector2((self.finishBox%width + prefs.START_X)*prefs.CELL_SIZE, (self.finishBox/width + prefs.START_Y)*prefs.CELL_SIZE)
		elif endAnim == 1:
			# top/bottom/top/bottom...
			var y = self.finishBox/width
			if y % 2 == 0:
				boxPos = Vector2((prefs.END_X - 1 - self.finishBox%width)*prefs.CELL_SIZE, (y/2 + prefs.START_Y)*prefs.CELL_SIZE)
			else:
				boxPos = Vector2((self.finishBox%width + prefs.START_X)*prefs.CELL_SIZE, (prefs.END_Y - 1 - y/2)*prefs.CELL_SIZE)
		elif endAnim == 2:
			# bottom/top/bottom/top
			var y = self.finishBox/width
			if y % 2 == 1:
				boxPos = Vector2((prefs.END_X - 1 - self.finishBox%width)*prefs.CELL_SIZE, (y/2 + prefs.START_Y)*prefs.CELL_SIZE)
			else:
				boxPos = Vector2((self.finishBox%width + prefs.START_X)*prefs.CELL_SIZE, (prefs.END_Y - 1 - y/2)*prefs.CELL_SIZE)
		elif endAnim == 3:
			# From bottom to top
			boxPos = Vector2((prefs.END_X - 1 - self.finishBox%width)*prefs.CELL_SIZE, (prefs.END_Y - 1 - self.finishBox/width)*prefs.CELL_SIZE)
		for p in players:
			rpc_id(p, "spawn_final_box", boxPos)
		self.spawn_final_box(boxPos)
	self.finishBox += 1

func start_end():
	if not is_network_master():
		return
	finishBox = 0
	endAnim = randi()%4
	var duration = 1.0/(float((prefs.END_X - prefs.START_X)*(prefs.END_Y - prefs.START_Y))/float(prefs.END_ANIM))
	self.timerEnd.connect("timeout", self, "spawn_end_box")
	add_child(self.timerEnd)
	self.timerEnd.start(duration)

func close_current_game():
	var current_game = get_tree().get_root().get_node("Game")
	if current_game:
		current_game._on_LeaveButton_pressed()

func check_winner():
	if get_tree().get_root().get_node("Game"):
		self.currentWorld.check_winner()
	else:
		self.timerFinish.stop()
		self.timerEnd.stop()

sync func in_lobby(id):
	self.in_lobby += 1
	if self.in_lobby == len(self.players) && host_master:
		get_tree().get_root().get_node("Lobby").get_node("RoomLobby").get_node("Start").show()

remote func pre_start_game(player_data, boxes, track):
	# Change scene.
	self.currentWorld = load("res://src/Game.tscn").instance()
	self.currentWorld.set_script(mainScript)
	# Spawn players
	for data in player_data:
		self.currentWorld.spawn_player(data, player_data[data])
	# Spawn cubes
	for box in boxes:
		self.currentWorld.spawn_box(box)
	# Show game
	get_tree().get_root().add_child(self.currentWorld)
	get_tree().get_root().get_node("Lobby").show_game()

	self.timerFinish.connect("timeout", self, "check_winner")
	add_child(self.timerFinish)
	self.timerFinish.start(0.5)
	self.in_lobby = 0
	self.currentWorld.start_track(track)

func lobby_shown():
	rpc("in_lobby", get_tree().get_network_unique_id())

remote func spawn_item(position, type):
	var root = get_tree().get_root()
	var tileMap = root.get_node("Game").get_node("Map")
	if not tileMap:
		return
	var tilePos = tileMap.world_to_map(position)
	var final_pos = (tilePos * prefs.CELL_SIZE) + Vector2(prefs.CELL_SIZE/2, -prefs.CELL_SIZE/2)
	self.currentWorld.spawn_item(final_pos, type)

func new_item(position):
	var type = randi()%6
	for p in players:
		rpc_id(p, "spawn_item", position, type)
	spawn_item(position, type)

func _ready():
	get_tree().connect("network_peer_connected", self, "_player_connected")
	get_tree().connect("network_peer_disconnected", self,"_player_disconnected")
	get_tree().connect("connected_to_server", self, "_connected_ok")
	get_tree().connect("connection_failed", self, "_connected_fail")
	get_tree().connect("server_disconnected", self, "_server_disconnected")