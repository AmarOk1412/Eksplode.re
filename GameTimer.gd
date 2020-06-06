extends Label

var time = 0
var total = 180
const prefs = preload("res://Utils/constant.gd")

func _process(delta):
	var diff = total - time
	var s = diff % 60
	var m = diff / 60
	var str_m = str(m) if m > 9 else "0" + str(m)
	var str_s = str(s) if s > 9 else "0" + str(s)
	set_text(str_m + ":" + str_s)

func _on_ms_timeout():
	if time < total:
		time += 1
	if total - time == prefs.END_ANIM:
		gamestate.start_end()