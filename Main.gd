extends Node

onready var destructibleBox = preload("res://DestructibleBox.tscn")
onready var boxScript = preload("res://DestructibleBox.gd")
var boxTexture = preload("res://Sprites/Box/box.png")
const prefs = preload("res://Utils/constant.gd")

func spawn_player(masterId, data):
	var pos = data[0]
	var type = data[1]
	var playerName = data[2]
	var player = load("res://Player.tscn")
	var playerScript = load("res://Player.gd")
	var p = player.instance()
	p.set_network_master(masterId)
	p.set_script(playerScript)
	p.add_to_group("Destroyable")
	p.add_to_group("Player")
	p.z_index = 3
	p.type = type
	p.position = pos
	p.playerName = playerName
	self.add_child(p)

func check_winner():
	var players = get_tree().get_nodes_in_group("Player")
	if players.size() == 1:
		$NodeWinningLabel/WinningLabel.text = "Game Over\n" + players[0].playerName + " Wins!"
	elif players.size() == 0:
		$NodeWinningLabel/WinningLabel.text = "Game Over"
	else:
		return
	$NodeWinningLabel.show()
	$GameTimer/ms.stop()

# Todo clean
func _ready():
	$GameTrack.play()
	$NodeWinningLabel.hide()
	for x in range(prefs.START_X, prefs.END_X):
		for y in range(prefs.START_Y, prefs.END_Y):
			if x%2 == 1 and y % 2 == 1:
				var box = destructibleBox.instance()
				box.z_index = 0
				box.get_node("Sprite").set_texture(boxTexture)
				box.add_to_group("Box")
				box.position = Vector2(x*prefs.CELL_SIZE, y*prefs.CELL_SIZE) + Vector2(prefs.CELL_SIZE/2, prefs.CELL_SIZE)
				var root = get_tree().get_root()
				self.add_child(box)
			elif (abs(x-prefs.START_X)<=1 or abs(x-(prefs.END_X-1))<=1) \
				and (abs(y-prefs.START_Y)<=1 or abs(y-(prefs.END_Y-1))<=1):
				continue
			elif randi()%3+1 != 2:
				var box = destructibleBox.instance()
				box.set_script(boxScript)
				box.z_index = 0
				box.add_to_group("Destroyable")
				box.add_to_group("Box")
				box.position = Vector2(x*prefs.CELL_SIZE, y*prefs.CELL_SIZE)+ Vector2(prefs.CELL_SIZE/2, prefs.CELL_SIZE)
				self.add_child(box)

func _on_LeaveButton_pressed():
	get_tree().get_root().get_node("Lobby").show()
	queue_free()
