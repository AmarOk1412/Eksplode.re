extends AnimatedSprite

func _ready():
	self.play("Boom")

func _on_Boom_animation_finished():
	queue_free()
