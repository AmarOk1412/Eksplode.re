extends Control

var player_name = ""

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

func _on_Quit_pressed():
	get_tree().quit()

func _on_Host_pressed():
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
	$RoomLobby/Start.hide()

func _on_connection_failed():
	$JoinPopup/ErrorLabel.set_text("Connection failed.")

func _on_JoinRoom_pressed():
	$JoinPopup/ErrorLabel.set_text("")
	var ip = $JoinPopup/Room.text
	if not ip.is_valid_ip_address():
		$JoinPopup/ErrorLabel.set_text("Invalid IP address!")
		return
	gamestate.join_game(ip, self.player_name)

func refresh_lobby():
	$RoomLobby/Player1.text = gamestate.player_name
	$RoomLobby/Player2.text = "Player 2"
	$RoomLobby/Player3.text = "Player 3"
	$RoomLobby/Player4.text = "Player 4"
	var p = 0
	for player in gamestate.players.values():
		if p == 0:
			$RoomLobby/Player2.text = player
		elif p == 1:
			$RoomLobby/Player3.text = player
		elif p == 2:
			$RoomLobby/Player4.text = player
		p += 1

func _on_Settings_pressed():
	$Settings/PlayerName.text = self.player_name
	$Settings.show()

func _on_Apply_pressed():
	self.player_name = $Settings/PlayerName.text
	$Settings.hide()

func _on_game_error(errtxt):
	$ErrorDialog.dialog_text = errtxt
	$ErrorDialog.popup_centered_minsize()

func _on_Start_pressed():
	gamestate.begin_game()
