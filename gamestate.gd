extends Node
const prefs = preload("res://Utils/constant.gd")
onready var mainScript = preload("res://Main.gd")
# Default game port. Can be any number between 1024 and 49151.
const DEFAULT_PORT = 14121

# Max number of players.
const MAX_PEERS = 4

# Names for remote players in id:name format.
var players = {}
var players_ready = []
# Name for my player.
var player_name = ""

# Signals
signal player_list_changed()
signal game_error(what)
signal connection_failed()
signal connection_succeeded()

# Callback from SceneTree.
func _player_connected(id):
	# Registration of a client beings here, tell the connected player that we are here.
	rpc_id(id, "register_player", player_name)

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

remote func register_player(new_player_name):
	var id = get_tree().get_rpc_sender_id()
	print("New player connected: " + str(id))
	players[id] = new_player_name
	emit_signal("player_list_changed")

func unregister_player(id):
	players.erase(id)
	print("Player disconnected: " + str(id))
	emit_signal("player_list_changed")

func host_game(new_player_name):
	player_name = new_player_name
	var host = NetworkedMultiplayerENet.new()
	host.create_server(DEFAULT_PORT, MAX_PEERS)
	get_tree().set_network_peer(host)

func join_game(ip, new_player_name):
	player_name = new_player_name
	var client = NetworkedMultiplayerENet.new()
	client.create_client(ip, DEFAULT_PORT)
	get_tree().set_network_peer(client)

func begin_game():
	assert(get_tree().is_network_server())
	var player_types = ["", "Red_", "Dark_", "Pink_"]
	
	for p in players:
		rpc_id(p, "pre_start_game", player_types)
	pre_start_game(player_types)

remote func pre_start_game(player_types):
	# Change scene.
	var world = load("res://Main.tscn").instance()
	world.set_script(mainScript)
	# Spawn players
	world.spawn_player(Vector2(prefs.START_X * prefs.CELL_SIZE, prefs.START_Y * prefs.CELL_SIZE) + Vector2(30, 30), player_types[0])
	world.spawn_player(Vector2(prefs.START_X * prefs.CELL_SIZE, prefs.END_Y * prefs.CELL_SIZE) + Vector2(30, -30), player_types[1])
	world.spawn_player(Vector2(prefs.END_X * prefs.CELL_SIZE, prefs.START_Y * prefs.CELL_SIZE) + Vector2(-30, 30), player_types[2])
	world.spawn_player(Vector2(prefs.END_X * prefs.CELL_SIZE, prefs.END_Y * prefs.CELL_SIZE) + Vector2(-30, -30), player_types[3])
	# Show game
	get_tree().get_root().add_child(world)
	get_tree().get_root().get_node("Lobby").hide()

func _ready():
	get_tree().connect("network_peer_connected", self, "_player_connected")
	get_tree().connect("network_peer_disconnected", self,"_player_disconnected")
	get_tree().connect("connected_to_server", self, "_connected_ok")
	get_tree().connect("connection_failed", self, "_connected_fail")
	get_tree().connect("server_disconnected", self, "_server_disconnected")