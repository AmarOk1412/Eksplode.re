extends Area2D

var malusTexture = preload("res://Sprites/Items/malus.png")
var bluegloveTexture = preload("res://Sprites/Items/blueglove.png")
var flameTexture = preload("res://Sprites/Items/flame.png")
var moreBombTexture = preload("res://Sprites/Items/moreBomb.png")
var redgloveTexture = preload("res://Sprites/Items/redglove.png")
var speedTexture = preload("res://Sprites/Items/speed.png")

# Declare member variables here. Examples:
# var a = 2
# var b = "text"

# Called when the node enters the scene tree for the first time.
func _ready():
	var type = randi()%2
	var subType = randi()%5
	if type == 0:
		$Sprite.set_texture(malusTexture)
	else:
		if subType == 0:
			$Sprite.set_texture(bluegloveTexture)
		elif subType == 1:
			$Sprite.set_texture(flameTexture)
		elif subType == 2:
			$Sprite.set_texture(moreBombTexture)
		elif subType == 3:
			$Sprite.set_texture(redgloveTexture)
		elif subType == 4:
			$Sprite.set_texture(speedTexture)

func explode():
	queue_free()

func on_player_entered(body):
	print("OK")
	queue_free()
