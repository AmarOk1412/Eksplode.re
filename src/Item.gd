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

extends Area2D

var malusTexture = preload("res://Media/Sprites/Items/malus.png")
var bluegloveTexture = preload("res://Media/Sprites/Items/blueglove.png")
var flameTexture = preload("res://Media/Sprites/Items/flame.png")
var moreBombTexture = preload("res://Media/Sprites/Items/moreBomb.png")
var redgloveTexture = preload("res://Media/Sprites/Items/redglove.png")
var speedTexture = preload("res://Media/Sprites/Items/speed.png")

var type = -1

# Called when the node enters the scene tree for the first time.
func set_type(new_type):
	type = new_type
	if type == 0:
		$Sprite.set_texture(bluegloveTexture)
	elif type == 1:
		$Sprite.set_texture(flameTexture)
	elif type == 2:
		$Sprite.set_texture(moreBombTexture)
	elif type == 3:
		$Sprite.set_texture(redgloveTexture)
	elif type == 4:
		$Sprite.set_texture(speedTexture)
	elif type == 5:
		$Sprite.set_texture(malusTexture)

func explode():
	queue_free()

func on_player_entered(body):
	if body.is_in_group("Bomb") and body.movingVec == Vector2():
		queue_free()
	elif body.is_in_group("Player"):
		if type == 0:
			body.repelBombs = true
		elif type == 1:
			body.radius += 1
		elif type == 2:
			body.bombs += 1
		elif type == 3:
			body.pushBombs = true
		elif type == 4:
			body.speed += 100
		elif type == 5:
			body.affect()
		queue_free()
