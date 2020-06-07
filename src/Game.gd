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

var destructibleBox = preload("res://src/DestructibleBox.tscn")
var boxScript = load("res://src/DestructibleBox.gd")
var boxTexture = preload("res://Media/Sprites/Box/box.png")
var itemPacked = preload("res://src/Item.tscn")
var itemScript = preload("res://src/Item.gd")

func spawn_player(masterId, data):
	var pos = data[0]
	var type = data[1]
	var playerName = data[2]
	var player = load("res://src/Player.tscn")
	var playerScript = load("res://src/Player.gd")
	var p = player.instance()
	p.set_network_master(masterId)
	p.set_script(playerScript)
	p.add_to_group("Destroyable")
	p.add_to_group("Player")
	p.z_index = 3
	p.type = type
	p.position = pos
	p.playerName = playerName
	p.puppet_pos = pos
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

func spawn_item(position, type):
	var item = itemPacked.instance()
	item.set_script(itemScript)
	item.add_to_group("Destroyable")
	item.set_type(type)
	item.z_index = 2
	item.position = position
	self.add_child(item)

func check_winner():
	var players = get_tree().get_nodes_in_group("Player")
	if players.size() == 1:
		for p in players:
			p.finished = true
		$NodeWinningLabel/WinningLabel.text = "Game Over\n" + players[0].playerName + " Wins!"
	elif players.size() == 0:
		$NodeWinningLabel/WinningLabel.text = "Game Over"
	else:
		return
	$NodeWinningLabel.show()
	$GameTimer/ms.stop()
	gamestate.timerEnd.stop()

func _ready():
	$GameTrack.play()
	$NodeWinningLabel.hide()
	$PushButton.hide()
	if OS.get_name() != "Android":
		$DropButton.hide()

func start_track(track):
	$GameTrack.stop()
	var stream = load("res://Media/Sounds/track" + str(track) + ".wav")
	$GameTrack.stream = stream
	$GameTrack.play()

func _on_LeaveButton_pressed():
	get_tree().get_root().get_node("Lobby").show_lobby()
	queue_free()

func _on_DropButton_pressed():
	for player in get_tree().get_nodes_in_group("Player"):
		if get_tree().get_network_unique_id() == player.get_network_master():
			player.announce_drop()

func _on_PushButton_pressed():
	for player in get_tree().get_nodes_in_group("Player"):
		if get_tree().get_network_unique_id() == player.get_network_master():
			player.announce_push()
