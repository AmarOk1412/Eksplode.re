extends Area2D

var malusTexture = preload("res://Sprites/Items/malus.png")
var bluegloveTexture = preload("res://Sprites/Items/blueglove.png")
var flameTexture = preload("res://Sprites/Items/flame.png")
var moreBombTexture = preload("res://Sprites/Items/moreBomb.png")
var redgloveTexture = preload("res://Sprites/Items/redglove.png")
var speedTexture = preload("res://Sprites/Items/speed.png")

var type = randi()%6

# Called when the node enters the scene tree for the first time.
# TODO clean with enums
func _ready():
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
