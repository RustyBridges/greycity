//! Fire: ignites, spreads through fuel, burns people, gets put out (sometimes).

use crate::map::{BuildingKind, Furniture, Pos, Terrain, DIRS8};
use crate::path;
use crate::world::{LogKind, World};
use rand::seq::SliceRandom;
use rand::Rng;

pub fn ignite(w: &mut World, p: Pos) -> bool {
    if !w.map.in_bounds(p) {
        return false;
    }
    let t = w.map.get_mut(p);
    if t.fire > 0 {
        return false;
    }
    if t.fuel == 0 {
        if t.furn == Furniture::Stove {
            t.fuel = 12; // grease
        } else {
            return false;
        }
    }
    t.fire = 3;
    w.burning.push(p);
    w.stats.fires += 1;
    true
}

pub fn extinguish(w: &mut World, at: Pos) {
    for dy in -1..=1 {
        for dx in -1..=1 {
            let q = at.add(dx, dy);
            if w.map.in_bounds(q) {
                w.map.get_mut(q).fire = 0;
            }
        }
    }
}

/// The nearest visible fire within `r` tiles.
pub fn fire_near(w: &World, from: Pos, r: i32) -> Option<Pos> {
    w.burning
        .iter()
        .filter(|b| w.map.get(**b).fire > 0 && b.dist(from) <= r)
        .filter(|b| path::los(&w.map, from, **b))
        .min_by_key(|b| b.dist(from))
        .copied()
}

pub fn nearest_burning(w: &World, from: Pos) -> Option<Pos> {
    w.burning.iter().filter(|b| w.map.get(**b).fire > 0).min_by_key(|b| b.dist(from)).copied()
}

/// A flammable spot inside some building, for those who hear the hum.
pub fn arson_target(w: &mut World) -> Option<Pos> {
    let ids: Vec<usize> = w.map.buildings.iter().filter(|b| b.kind != BuildingKind::Park).map(|b| b.id).collect();
    let b = *ids.choose(&mut w.rng)?;
    random_fuel_in(w, b)
}

pub fn random_fuel_in(w: &mut World, b: usize) -> Option<Pos> {
    let bl = &w.map.buildings[b];
    let mut c = vec![];
    for y in bl.y..bl.y + bl.h {
        for x in bl.x..bl.x + bl.w {
            let p = Pos::new(x, y);
            let t = w.map.get(p);
            if t.furn != Furniture::None && t.fuel > 0 {
                c.push(p);
            }
        }
    }
    c.choose(&mut w.rng).copied()
}

pub fn step(w: &mut World) {
    if w.burning.is_empty() {
        w.fire_alarm = false;
        return;
    }
    let list = std::mem::take(&mut w.burning);
    let mut keep = Vec::with_capacity(list.len());
    let mut new = vec![];
    for p in list {
        let t = *w.map.get(p);
        if t.fire == 0 {
            continue;
        }
        let tile = w.map.get_mut(p);
        if tile.fuel > 0 {
            tile.fuel -= 1;
            tile.fire = (tile.fire + 1).min(10);
        } else {
            tile.fire = tile.fire.saturating_sub(2);
        }
        if tile.fire == 0 {
            if tile.furn != Furniture::None && tile.furn != Furniture::Payphone {
                tile.furn = Furniture::None;
            }
            if matches!(tile.terrain, Terrain::Wood | Terrain::Carpet | Terrain::Door | Terrain::Grass) {
                tile.terrain = Terrain::Ash;
            }
            if let Some(b) = w.map.building_at(p) {
                w.map.buildings[b].burned += 1;
            }
            continue;
        }
        let intensity = tile.fire as f32;
        for (k, (dx, dy)) in DIRS8.iter().enumerate() {
            let q = p.add(*dx, *dy);
            if !w.map.in_bounds(q) {
                continue;
            }
            let qt = w.map.get(q);
            if qt.fuel > 0 && qt.fire == 0 {
                let chance = intensity * 0.0045 * if k >= 4 { 0.5 } else { 1.0 };
                if w.rng.gen::<f32>() < chance {
                    w.map.get_mut(q).fire = 1;
                    new.push(q);
                }
            }
        }
        keep.push(p);
    }
    keep.extend(new);
    w.burning = keep;

    // People standing in fire.
    let now = w.turn;
    for i in 0..w.people.len() {
        let p = &w.people[i];
        if !p.alive || p.jailed_until > now {
            continue;
        }
        let f = w.map.get(p.pos).fire;
        if f == 0 {
            continue;
        }
        let part = w.rng.gen_range(0..6);
        let dmg = 1 + f as i32 / 4;
        let q = &mut w.people[i];
        q.body.hp[part] -= dmg;
        q.stress = (q.stress + 8.0).min(100.0);
        q.sleeping = false;
        let pos = q.pos;
        let dead = q.body.hp[0] <= 0 || q.body.hp[1] <= 0;
        if dead {
            w.kill(i, "burned to death".into(), None);
        } else if now - w.people[i].last_scream > 6 {
            w.people[i].last_scream = now;
            let n = w.cname(i);
            let verb = if i == w.player { "are" } else { "is" };
            w.say(Some(pos), &[i], LogKind::Danger, format!("{} {} on fire!", n, verb));
        }
    }

    // Somebody calls it in.
    if !w.fire_alarm {
        let caller = (0..w.people.len()).find(|&j| {
            let p = &w.people[j];
            p.awake(now) && !p.hum && fire_near(w, p.pos, 8).is_some()
        });
        if let Some(c) = caller {
            w.fire_alarm = true;
            let pos = w.people[c].pos;
            let n = w.cname(c);
            let where_ = fire_near(w, pos, 8)
                .and_then(|f| w.map.building_at(f))
                .map(|b| w.map.buildings[b].name.clone())
                .unwrap_or_else(|| "the street".into());
            let verb = if c == w.player { "call" } else { "calls" };
            let close = fire_near(w, pos, 8).map(|fp| fp.dist(w.people[w.player].pos) <= 15).unwrap_or(false);
            let kind = if close { LogKind::Danger } else { LogKind::Bad };
            w.say(None, &[c], kind, format!("{} {} the fire department: there's a fire at {}.", n, verb, where_));
        }
    }
}
