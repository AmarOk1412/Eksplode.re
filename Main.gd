extends Node

const prefs = preload("res://Utils/constant.gd")

var destructibleBox = preload("res://DestructibleBox.tscn")
var boxScript = preload("res://DestructibleBox.gd")
var boxTexture = preload("res://Sprites/Box/box.png")

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
	$ObjectSort.add_child(p)

func spawn_box(box_data):
	var box = destructibleBox.instance()
	box.z_index = 0
	if box_data[0]:
		box.add_to_group("Destroyable")
		box.set_script(boxScript)
	else:
		box.get_node("Sprite").set_texture(boxTexture)
	box.add_to_group("Box")
	box.position = box_data[1]
	$ObjectSort.add_child(box)

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
	gamestate.timerEnd.stop()

# Todo clean
func _ready():
	$GameTrack.play()
	$NodeWinningLabel.hide()


func _on_LeaveButton_pressed():
	get_tree().get_root().get_node("Lobby").show_lobby()
	queue_free()
