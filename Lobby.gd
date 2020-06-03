extends Control

var player_name = ""
var possibleTypes = ["", "Red_", "Dark_", "Pink_"]

func _ready():
	gamestate.connect("player_list_changed", self, "refresh_lobby")
	gamestate.connect("game_error", self, "_on_game_error")
	gamestate.connect("connection_failed", self, "_on_connection_failed")
	gamestate.connect("connection_succeeded", self, "_on_connection_success")
	# Set the player name according to the system username. Fallback to the path.
	if OS.has_environment("USERNAME"):
		self.player_name = OS.get_environment("USERNAME")
	else:
		var desktop_path = OS.get_system_dir(0).replace("\\", "/").split("/")
		self.player_name = desktop_path[desktop_path.size() - 2]

	$MainScreen.show()
	$RoomLobby.hide()
	$JoinPopup.hide()
	$Settings.hide()
	$LobbySound.play()

func _on_Quit_pressed():
	get_tree().quit()

func _on_Host_pressed():
	$MainScreen.hide()
	$RoomLobby.show()
	$RoomLobby/Start.hide()
	gamestate.host_game(self.player_name)
	refresh_lobby()

func _on_Join_pressed():
	$JoinPopup.show()

func _on_JoinCancel_pressed():
	$JoinPopup.hide()

func _on_connection_success():
	$MainScreen.hide()
	$JoinPopup.hide()
	$RoomLobby.show()

func _on_connection_failed():
	$JoinPopup/ErrorLabel.set_text("Connection failed.")

func _on_JoinRoom_pressed():
	$JoinPopup/ErrorLabel.set_text("")
	$RoomLobby/Start.hide()
	var address = $JoinPopup/Room.text.split(":")
	if len(address) == 2:
		gamestate.join_game(self.player_name, address[0], address[1])
	else:
		gamestate.join_game(self.player_name, address[0])

func _input(ev):
	if len(gamestate.players) == 0:
		return
	if get_tree().get_network_unique_id() == 0:
		return
	var current_style = gamestate.players[get_tree().get_network_unique_id()][1]
	var idx = self.possibleTypes.find(current_style)
	if Input.is_action_just_pressed("ui_left"):
		idx = (idx - 1 + len(self.possibleTypes)) % len(self.possibleTypes)
	elif Input.is_action_just_pressed("ui_right"):
		idx = (idx + 1 + len(self.possibleTypes)) % len(self.possibleTypes)
	rpc("change_character", get_tree().get_network_unique_id(), self.possibleTypes[idx])

sync func change_character(id, new_character):
	gamestate.players[id][1] = new_character
	refresh_lobby()

func refresh_lobby():
	var p = 0
	var playersKeys = gamestate.players.keys()
	playersKeys.sort()
	if len(playersKeys) != 4:
		$RoomLobby/Player4Sprite.hide()
		$RoomLobby/Player4.hide()
	if len(playersKeys) != 3:
		$RoomLobby/Player3Sprite.hide()
		$RoomLobby/Player3.hide()
	if len(playersKeys) != 2:
		$RoomLobby/Player2Sprite.hide()
		$RoomLobby/Player2.hide()
	for key in playersKeys:
		var player = gamestate.players[key]
		if p == 0:
			$RoomLobby/Player1.text = player[0]
			$RoomLobby/Player1.show()
			$RoomLobby/Player1Sprite.set_texture(load("res://Sprites/Lobby/Players/Perso_" + player[1] + ".png"))
			$RoomLobby/Player1Sprite.show()
		elif p == 1:
			$RoomLobby/Player2.text = player[0]
			$RoomLobby/Player2.show()
			$RoomLobby/Player2Sprite.set_texture(load("res://Sprites/Lobby/Players/Perso_" + player[1] + ".png"))
			$RoomLobby/Player2Sprite.show()
		elif p == 2:
			$RoomLobby/Player3.text = player[0]
			$RoomLobby/Player3.show()
			$RoomLobby/Player3Sprite.set_texture(load("res://Sprites/Lobby/Players/Perso_" + player[1] + ".png"))
			$RoomLobby/Player3Sprite.show()
		elif p == 3:
			$RoomLobby/Player4.text = player[0]
			$RoomLobby/Player4.show()
			$RoomLobby/Player4Sprite.set_texture(load("res://Sprites/Lobby/Players/Perso_" + player[1] + ".png"))
			$RoomLobby/Player4Sprite.show()
		p += 1

func _on_Settings_pressed():
	$Settings/PlayerName.text = self.player_name
	$Settings.show()

func _on_Apply_pressed():
	self.player_name = $Settings/PlayerName.text
	$Settings.hide()

func _on_game_error(errtxt):
	gamestate.close_current_game()
	$ErrorDialog.dialog_text = errtxt
	$ErrorDialog.popup_centered_minsize()
	$RoomLobby.hide()
	$MainScreen.show()

func _on_Start_pressed():
	gamestate.begin_game()

func show_lobby():
	$LobbySound.play()
	self.show()
	$RoomLobby/Start.hide()
	gamestate.lobby_shown()

func show_game():
	$LobbySound.stop()
	self.hide()

func _on_Leave_pressed():
	$RoomLobby.hide()
	$MainScreen.show()
	get_tree().set_network_peer(null)

func _on_LobbySound_finished():
	if self.is_visible_in_tree():
		$LobbySound.play()
