//! A* pathfinding and line-of-sight / field-of-view helpers.

use crate::map::{Map, Pos, DIRS8};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Returns the path from `start` to `goal` (excluding start), stored *reversed*
/// so callers can `pop()` the next step. `goal` itself may be unwalkable
/// (e.g. a counter) in which case the path ends next to it.
pub fn astar(map: &Map, start: Pos, goal: Pos) -> Option<Vec<Pos>> {
    if start == goal {
        return Some(vec![]);
    }
    let n = (map.w * map.h) as usize;
    let mut g = vec![u32::MAX; n];
    let mut came = vec![u32::MAX; n];
    let mut heap = BinaryHeap::new();
    let goal_walkable = map.walkable(goal);
    let h = |p: Pos| {
        let dx = (p.x - goal.x).unsigned_abs();
        let dy = (p.y - goal.y).unsigned_abs();
        10 * dx.max(dy) + 4 * dx.min(dy)
    };
    let si = map.idx(start);
    g[si] = 0;
    heap.push(Reverse((h(start), si as u32)));
    let mut found = None;
    let mut expanded = 0;
    while let Some(Reverse((_, ci))) = heap.pop() {
        let ci = ci as usize;
        let c = Pos::new(ci as i32 % map.w, ci as i32 / map.w);
        if c == goal || (!goal_walkable && c.adjacent(goal)) {
            found = Some(ci);
            break;
        }
        expanded += 1;
        if expanded > n {
            break;
        }
        for (k, (dx, dy)) in DIRS8.iter().enumerate() {
            let np = c.add(*dx, *dy);
            if !map.walkable(np) {
                continue;
            }
            let diag = k >= 4;
            if diag && (!map.walkable(c.add(*dx, 0)) || !map.walkable(c.add(0, *dy))) {
                continue;
            }
            let t = map.get(np);
            let mut cost = if diag { 14 } else { 10 };
            if t.fire > 0 {
                cost += 400;
            }
            let ni = map.idx(np);
            let ng = g[ci] + cost;
            if ng < g[ni] {
                g[ni] = ng;
                came[ni] = ci as u32;
                heap.push(Reverse((ng + h(np), ni as u32)));
            }
        }
    }
    let mut cur = found?;
    let mut out = vec![];
    while cur != si {
        out.push(Pos::new(cur as i32 % map.w, cur as i32 / map.w));
        cur = came[cur] as usize;
    }
    Some(out)
}

/// Bresenham line; true if nothing opaque lies strictly between a and b.
pub fn los(map: &Map, a: Pos, b: Pos) -> bool {
    let (mut x, mut y) = (a.x, a.y);
    let dx = (b.x - a.x).abs();
    let dy = -(b.y - a.y).abs();
    let sx = if a.x < b.x { 1 } else { -1 };
    let sy = if a.y < b.y { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x == b.x && y == b.y {
            return true;
        }
        if (x, y) != (a.x, a.y) && !map.transparent(Pos::new(x, y)) {
            return false;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Simple ray-cast field of view.
pub fn fov(map: &Map, origin: Pos, radius: i32, out: &mut [bool]) {
    out.iter_mut().for_each(|v| *v = false);
    out[map.idx(origin)] = true;
    let steps = (radius * 8).max(64);
    for i in 0..steps {
        let ang = i as f32 / steps as f32 * std::f32::consts::TAU;
        let (dx, dy) = (ang.cos(), ang.sin());
        let (mut fx, mut fy) = (origin.x as f32 + 0.5, origin.y as f32 + 0.5);
        for _ in 0..radius {
            fx += dx;
            fy += dy;
            let p = Pos::new(fx.floor() as i32, fy.floor() as i32);
            if !map.in_bounds(p) {
                break;
            }
            out[map.idx(p)] = true;
            if !map.transparent(p) {
                break;
            }
        }
    }
}
