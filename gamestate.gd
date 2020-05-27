extends Node
const prefs = preload("res://Utils/constant.gd")
onready var mainScript = load("res://Main.gd")
# Default game port. Can be any number between 1024 and 49151.
const DEFAULT_PORT = 14121

# Max number of players.
const MAX_PEERS = 4

# Names for remote players in id:name format.
var players = {}

# Signals
signal player_list_changed()
signal game_error(what)
signal connection_failed()
signal connection_succeeded()

# Timer finish
var timerFinish = Timer.new()
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
	emit_signal("player_list_changed")

func unregister_player(id):
	players.erase(id)
	print("Player disconnected: " + str(id))
	emit_signal("player_list_changed")

func host_game(new_player_name):
	var host = NetworkedMultiplayerENet.new()
	host.create_server(DEFAULT_PORT, MAX_PEERS)
	get_tree().set_network_peer(host)
	players[1] = [new_player_name, ""]

func join_game(ip, new_player_name):
	players = {}
	var client = NetworkedMultiplayerENet.new()
	client.create_client(ip, DEFAULT_PORT)
	get_tree().set_network_peer(client)
	players[get_tree().get_network_unique_id()] =  [new_player_name, ""]

func begin_game():
	assert(get_tree().is_network_server())
	var player_data = {}
	var idx = 0
	for p in players:
		if idx == 0:
			player_data[p] = [ \
				Vector2(prefs.START_X * prefs.CELL_SIZE, prefs.START_Y * prefs.CELL_SIZE) + Vector2(30, 30), \
				players[p][1],
				players[p][0]]
		elif idx == 1:
			player_data[p] = [
			Vector2(prefs.START_X * prefs.CELL_SIZE, prefs.END_Y * prefs.CELL_SIZE) + Vector2(30, -30),
			players[p][1],
			players[p][0]]
		elif idx == 2:
			player_data[p] = [
			Vector2(prefs.END_X * prefs.CELL_SIZE, prefs.START_Y * prefs.CELL_SIZE) + Vector2(-30, 30), \
			players[p][1],
			players[p][0]]
		elif idx == 3:
			player_data[p] = [
			Vector2(prefs.END_X * prefs.CELL_SIZE, prefs.END_Y * prefs.CELL_SIZE) + Vector2(-30, -30), \
			players[p][1],
			players[p][0]]
		idx += 1

	for p in players:
		rpc_id(p, "pre_start_game", player_data)
	pre_start_game(player_data)

func check_winner():
	if self.currentWorld:
		self.currentWorld.check_winner()

remote func pre_start_game(player_data):
	# Change scene.
	self.currentWorld = load("res://Main.tscn").instance()
	self.currentWorld.set_script(mainScript)
	# Spawn players
	for data in player_data:
		self.currentWorld.spawn_player(data, player_data[data])
	# Show game
	get_tree().get_root().add_child(self.currentWorld)
	get_tree().get_root().get_node("Lobby").hide()
	
	self.timerFinish.connect("timeout", self, "check_winner")
	add_child(self.timerFinish)
	self.timerFinish.start(0.5)

func _ready():
	get_tree().connect("network_peer_connected", self, "_player_connected")
	get_tree().connect("network_peer_disconnected", self,"_player_disconnected")
	get_tree().connect("connected_to_server", self, "_connected_ok")
	get_tree().connect("connection_failed", self, "_connected_fail")
	get_tree().connect("server_disconnected", self, "_server_disconnected")