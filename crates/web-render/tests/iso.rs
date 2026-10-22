use bomberust_web_render::iso::{arena_bounds, fit, project, unproject, MARGIN, WALL_H};

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn projection_follows_the_two_to_one_formula() {
    assert_eq!(project(0.0, 0.0, 0.0), (0.0, 0.0));
    assert_eq!(project(1.0, 0.0, 0.0), (32.0, 16.0));
    assert_eq!(project(0.0, 1.0, 0.0), (-32.0, 16.0));
    assert_eq!(project(2.0, 3.0, 0.0), (-32.0, 80.0));
    assert_eq!(
        project(0.0, 0.0, 10.0),
        (0.0, -10.0),
        "height lifts the point up the screen"
    );
}

#[test]
fn unproject_inverts_the_ground_plane() {
    for (x, y) in [(0.0, 0.0), (3.5, 2.25), (14.0, 12.0), (7.0, 0.5)] {
        let (sx, sy) = project(x, y, 0.0);
        let (ux, uy) = unproject(sx, sy);
        assert!(close(ux, x) && close(uy, y), "({x},{y}) -> ({ux},{uy})");
    }
}

#[test]
fn the_arena_is_centred_and_fits_in_any_canvas() {
    for canvas in [
        (400.0, 300.0),
        (800.0, 600.0),
        (1920.0, 1080.0),
        (300.0, 900.0),
    ] {
        for (w, h) in [(15u8, 13u8), (7, 7)] {
            let view = fit(w, h, canvas);
            assert!(view.scale > 0.0);
            let (x0, y0, x1, y1) = arena_bounds(w, h);
            let (ax, ay) = view.apply((x0, y0));
            let (bx, by) = view.apply((x1, y1));
            assert!(
                close((ax + bx) / 2.0, canvas.0 / 2.0),
                "{canvas:?} {w}x{h} centred x"
            );
            assert!(
                close((ay + by) / 2.0, canvas.1 / 2.0),
                "{canvas:?} {w}x{h} centred y"
            );
            assert!(
                ax >= MARGIN - 0.01 && bx <= canvas.0 - MARGIN + 0.01,
                "{canvas:?} fits x"
            );
            assert!(
                ay >= MARGIN - 0.01 && by <= canvas.1 - MARGIN + 0.01,
                "{canvas:?} fits y"
            );
        }
    }
}

#[test]
fn bounds_include_the_wall_height() {
    let (_, min_y, _, max_y) = arena_bounds(15, 13);
    assert!(
        close(min_y, -WALL_H),
        "top edge is the tallest wall at the far corner"
    );
    assert!(close(max_y, project(15.0, 13.0, 0.0).1));
}

#[test]
fn view_round_trips_between_canvas_and_tiles() {
    let view = fit(15, 13, (800.0, 600.0));
    let (px, py) = view.project(4.0, 6.0, 0.0);
    let (x, y) = view.unproject(px, py);
    assert!(close(x, 4.0) && close(y, 6.0));
}

#[test]
fn a_degenerate_canvas_never_gives_nan() {
    for canvas in [(0.0, 0.0), (10.0, 10.0), (f32::NAN, 100.0)] {
        let v = fit(15, 13, canvas);
        assert!(v.scale.is_finite() && v.offset_x.is_finite() && v.offset_y.is_finite());
    }
}
