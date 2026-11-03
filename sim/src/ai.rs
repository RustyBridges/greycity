//! NPC decision making. Each turn every person reacts to danger, then follows
//! (or picks) an intent: work, sleep, eat, drink, socialize, fight, flee...

use crate::combat;
use crate::fire;
use crate::map::{BuildingKind, Furniture, Pos, DIRS8};
use crate::path;
use crate::person::*;
use crate::social;
use crate::world::{LogKind, Role, World};
use rand::seq::SliceRandom;
use rand::Rng;

pub fn think(w: &mut World, i: usize) {
    let now = w.turn;
    if !w.people[i].alive {
        return;
    }
    if w.people[i].jailed_until > 0 && w.people[i].jailed_until <= now {
        combat::release(w, i);
    }
    if !w.people[i].can_act(now) {
        return;
    }
    let pos = w.people[i].pos;

    // Sleepers only wake for good reasons.
    if w.people[i].sleeping {
        let p = &w.people[i];
        let wake = (p.energy >= 97.0 && !sleep_window(w, i) || p.energy >= 100.0)
            || (p.job.is_some() && w.on_shift_at(i, now + 40) && p.energy > 40.0)
            || p.hostile_to.is_some()
            || p.hunger > 95.0
            || fire::fire_near(w, pos, 2).is_some();
        if !wake {
            return;
        }
        let p = &mut w.people[i];
        p.sleeping = false;
        p.intent = Intent::default();
    }

    // 1. Fire.
    if let Some(fp) = fire::fire_near(w, pos, 6) {
        if w.on_duty(i, Role::Firefighter) {
            if w.people[i].intent.kind != IntentKind::Extinguish {
                set_intent(w, i, IntentKind::Extinguish, fp, None);
            }
        } else if !matches!(w.people[i].intent.kind, IntentKind::Flee | IntentKind::Arson | IntentKind::Fight(_)) {
            start_flee(w, i, fp);
            if now - w.people[i].last_scream > 120 {
                w.people[i].last_scream = now;
                let n = w.people[i].name.clone();
                let lines = ["runs from the fire", "screams about the fire", "backs away from the flames, phone already out", "sprints away, abandoning a shoe"];
                let l = lines.choose(&mut w.rng).unwrap();
                w.say(Some(pos), &[i], LogKind::Bad, format!("{} {}.", n, l));
            }
        }
    }

    // 2. Someone is fighting me.
    if let Some(t) = w.people[i].hostile_to {
        let tp = &w.people[t];
        if tp.alive && tp.jailed_until <= now && !matches!(w.people[i].intent.kind, IntentKind::Fight(_) | IntentKind::Chase(_)) {
            let me = &w.people[i];
            let scared = me.has(Trait::Coward) || me.body.health_frac() < 0.4;
            if scared && w.role(i) != Role::Cop {
                if w.people[i].intent.kind != IntentKind::Flee {
                    let tpos = w.people[t].pos;
                    start_flee(w, i, tpos);
                }
            } else {
                set_intent(w, i, IntentKind::Fight(t), w.people[t].pos, None);
            }
        }
    }

    // 3. Duty calls.
    let kind = w.people[i].intent.kind;
    let interruptible = matches!(
        kind,
        IntentKind::None | IntentKind::Work | IntentKind::Wander | IntentKind::Loiter | IntentKind::Socialize | IntentKind::Eat | IntentKind::Drink
    );
    if interruptible && w.on_duty(i, Role::Cop) {
        if let Some(perp) = w.wanted() {
            if perp != i && w.people[perp].pos.dist(pos) < 60 {
                set_intent(w, i, IntentKind::Chase(perp), w.people[perp].pos, None);
                if w.chance(0.3) {
                    let (n, pn) = (w.people[i].name.clone(), w.name(perp));
                    w.say(Some(pos), &[i, perp], LogKind::Bad, format!("Officer {} goes looking for {}.", n, pn));
                }
            }
        }
    }
    if interruptible && w.fire_alarm && w.on_duty(i, Role::Firefighter) {
        if let Some(fp) = fire::nearest_burning(w, pos) {
            set_intent(w, i, IntentKind::Extinguish, fp, None);
        }
    }

    if matches!(w.people[i].intent.kind, IntentKind::None | IntentKind::Drink | IntentKind::Socialize | IntentKind::Loiter | IntentKind::Wander | IntentKind::Eat | IntentKind::Work) {
        grudge_check(w, i);
    }
    if w.people[i].intent.kind == IntentKind::None {
        choose(w, i);
    }
    if std::env::var("GC_TRACE").map(|v| v == w.people[i].name).unwrap_or(false) && now % 30 == 0 {
        let p = &w.people[i];
        eprintln!("{} {:02}:{:02} {:?} started={} pos={:?} tgt={:?} E={:.0} H={:.0} sleeping={}", p.name, (now%1440)/60, now%60, p.intent.kind, p.intent.started, p.pos, p.intent.target, p.energy, p.hunger, p.sleeping);
    }
    execute(w, i);
}

/// Is it this person's "night"? Day workers sleep 23-06; night workers sleep
/// for eight hours after their shift ends.
pub fn sleep_window(w: &World, i: usize) -> bool {
    let hour = w.hour();
    let Some(j) = w.people[i].job else { return hour >= 23 || hour < 7 };
    let job = &w.jobs[j];
    let end = (job.start + job.hours) % 24;
    let late = job.start >= 14 || job.start + job.hours > 24;
    if !late {
        return hour >= 23 || hour < 6;
    }
    let since_end = (hour + 24 - end) % 24;
    since_end < 8
}

/// Old grudges. Drink and stress make people brave in the wrong way.
fn grudge_check(w: &mut World, i: usize) {
    let now = w.turn;
    let p = &w.people[i];
    let pos = p.pos;
    let volatile = p.drunk > 25.0 || p.stress > 65.0 || p.has(Trait::Hothead);
    if !volatile || p.has(Trait::Coward) {
        return;
    }
    if !w.chance(0.01) {
        return;
    }
    let p = &w.people[i];
    let enemy = p
        .opinions
        .iter()
        .filter(|(x, o)| **o <= -55 && w.people[**x].alive && w.people[**x].awake(now) && w.people[**x].jailed_until <= now)
        .map(|(x, _)| *x)
        .find(|x| w.people[*x].pos.dist(pos) <= 8 && path::los(&w.map, pos, w.people[*x].pos));
    let Some(x) = enemy else { return };
    let (a, b) = (w.people[i].name.clone(), w.name(x));
    if w.chance(0.5) {
        // Words first.
        w.say(Some(pos), &[i, x], LogKind::Bad, format!("{} spots {} and goes over to say something.", a, b));
        if pos.dist(w.people[x].pos) <= 1 {
            social::insult(w, i, x);
        } else {
            let t = w.people[x].pos;
            w.people[i].pos = {
                let q = path::astar(&w.map, pos, t).and_then(|mut pth| pth.pop());
                q.unwrap_or(pos)
            };
        }
        return;
    }
    w.say(Some(pos), &[i, x], LogKind::Danger, format!("{} storms over to {}, fists already closed.", a, b));
    let q = &mut w.people[i];
    q.hostile_to = Some(x);
    q.hostile_until = now + 40;
    set_intent(w, i, IntentKind::Fight(x), w.people[x].pos, None);
}

pub fn set_intent(w: &mut World, i: usize, kind: IntentKind, target: Pos, building: Option<usize>) {
    let p = &mut w.people[i];
    p.intent = Intent { kind, target, building, started: false, until: 0 };
    p.sleeping = false;
}

fn clear(w: &mut World, i: usize) {
    let p = &mut w.people[i];
    p.intent = Intent::default();
    p.sleeping = false;
}

fn start_flee(w: &mut World, i: usize, from: Pos) {
    let pos = w.people[i].pos;
    // Standing in fire? First, any step out of it.
    if w.map.get(pos).fire > 0 {
        let out = DIRS8.iter().map(|(dx, dy)| pos.add(*dx, *dy)).find(|q| w.map.walkable(*q) && w.map.get(*q).fire == 0);
        if let Some(o) = out {
            w.people[i].pos = o;
        }
    }
    let pos = w.people[i].pos;
    // Inside a building? Head for the nearest way out.
    if w.map.indoors(pos) {
        if let Some(b) = w.map.building_at(pos) {
            let exit = w.map.buildings[b].entrances.iter().copied().min_by_key(|e| e.dist(pos));
            if let Some(e) = exit {
                let bl = &w.map.buildings[b];
                let outside = [e.add(0, 1), e.add(0, -1), e.add(1, 0), e.add(-1, 0)]
                    .into_iter()
                    .find(|q| !bl.contains(*q) && w.map.walkable(*q));
                if let Some(o) = outside {
                    let now = w.turn;
                    let p = &mut w.people[i];
                    p.intent = Intent { kind: IntentKind::Flee, target: o, until: now + 60, ..Default::default() };
                    p.path.clear();
                    p.sleeping = false;
                    return;
                }
            }
        }
    }
    let (dx, dy) = ((pos.x - from.x).signum(), (pos.y - from.y).signum());
    let (dx, dy) = if dx == 0 && dy == 0 { (1, 0) } else { (dx, dy) };
    let mut target = pos;
    for k in (3..12).rev() {
        let t = pos.add(dx * k, dy * k);
        if w.map.walkable(t) && w.map.get(t).fire == 0 {
            target = t;
            break;
        }
    }
    if target == pos {
        // Just get outside.
        if let Some(b) = w.map.building_at(pos) {
            if let Some(e) = w.map.buildings[b].entrances.first() {
                target = e.add(0, 1);
            }
        }
    }
    let now = w.turn;
    let p = &mut w.people[i];
    p.intent = Intent { kind: IntentKind::Flee, target, until: now + 25, ..Default::default() };
    p.path.clear();
    p.sleeping = false;
}

/// Move one step toward `target`. Returns false if unreachable.
pub fn step_to(w: &mut World, i: usize, target: Pos, dynamic: bool) -> bool {
    let now = w.turn;
    let pos = w.people[i].pos;
    if pos == target {
        return true;
    }
    if !w.people[i].body.legs_ok() && now % 3 != 0 {
        return true; // crawling
    }
    let need = {
        let p = &w.people[i];
        p.path.is_empty() || p.path_goal != target || (dynamic && w.rng.gen::<f32>() < 0.35)
    };
    if need {
        match path::astar(&w.map, pos, target) {
            Some(pth) => {
                let p = &mut w.people[i];
                p.path = pth;
                p.path_goal = target;
            }
            None => return false,
        }
    }
    let Some(next) = w.people[i].path.pop() else { return true };
    if !next.adjacent(pos) || !w.map.walkable(next) {
        w.people[i].path.clear();
        return true;
    }
    if w.map.get(next).fire > 0 && w.role(i) != Role::Firefighter && w.people[i].intent.kind != IntentKind::Arson {
        w.people[i].path.clear();
        return true;
    }
    // Don't walk into the player; wait instead.
    if next == w.people[w.player].pos && i != w.player && w.people[w.player].alive && w.rng.gen::<f32>() < 0.7 {
        w.people[i].path.push(next);
        return true;
    }
    w.people[i].pos = next;
    true
}

fn occupied_targets(w: &World, me: usize) -> Vec<Pos> {
    w.people
        .iter()
        .filter(|p| p.alive && p.id != me)
        .flat_map(|p| [p.pos, p.intent.target])
        .collect()
}

fn free_spot(w: &mut World, me: usize, spots: &[Pos]) -> Option<Pos> {
    let taken = occupied_targets(w, me);
    let mut free: Vec<Pos> = spots.iter().copied().filter(|s| !taken.contains(s)).collect();
    free.shuffle(&mut w.rng);
    free.first().copied()
}

fn random_nearby_walkable(w: &mut World, from: Pos, r: i32) -> Option<Pos> {
    for _ in 0..20 {
        let p = from.add(w.rng.gen_range(-r..=r), w.rng.gen_range(-r..=r));
        if w.map.walkable(p) && w.map.get(p).fire == 0 {
            return Some(p);
        }
    }
    None
}

fn go_clinic(w: &mut World, i: usize) -> bool {
    let Some(c) = w.map.first_of(BuildingKind::Clinic) else { return false };
    let beds = w.map.buildings[c].beds.clone();
    let seats = w.map.buildings[c].seats.clone();
    match free_spot(w, i, &beds).or_else(|| free_spot(w, i, &seats)) {
        Some(t) => {
            set_intent(w, i, IntentKind::Clinic, t, Some(c));
            w.people[i].last_clinic = w.turn;
            true
        }
        None => false,
    }
}

fn choose(w: &mut World, i: usize) {
    let now = w.turn;
    let hour = w.hour();
    let p = w.people[i].clone();
    let pos = p.pos;

    // The hum gets some people. It always has.
    if p.hum {
        if let Some(t) = fire::arson_target(w) {
            set_intent(w, i, IntentKind::Arson, t, w.map.building_at(t));
            let bn = w.map.building_at(t).map(|b| w.map.buildings[b].name.clone()).unwrap_or_default();
            w.say(Some(pos), &[i], LogKind::Weird, format!("{} walks toward {} with enormous calm.", p.name, bn));
            return;
        }
    }

    // Snack in the pocket?
    if p.hunger > 45.0 {
        if let Some(k) = p.inventory.iter().position(|x| *x == ItemKind::Snack) {
            let q = &mut w.people[i];
            q.inventory.remove(k);
            q.hunger = (q.hunger - 35.0).max(0.0);
        }
    }

    // Bleeding: that comes first.
    if p.body.bleeding > 0 && now - p.last_clinic > 120 {
        if go_clinic(w, i) {
            return;
        }
    }

    // Work.
    if p.job.is_some() && w.on_shift(i) {
        let skip = p.has(Trait::Lazy) && (w.day() + i as u64) % 6 == 0 || p.drunk > 70.0;
        if !skip {
            if let Some(spot) = p.work_spot {
                set_intent(w, i, IntentKind::Work, spot, p.workplace);
                return;
            }
        }
    }

    // Sleep.
    if p.energy < 25.0 || (sleep_window(w, i) && p.energy < 80.0) {
        let target = match p.bed {
            Some(b) => Some(b),
            None => w.map.first_of(BuildingKind::Park).and_then(|pk| {
                let seats = w.map.buildings[pk].seats.clone();
                free_spot(w, i, &seats)
            }),
        };
        set_intent(w, i, IntentKind::Sleep, target.unwrap_or(pos), None);
        return;
    }

    // Eat.
    if p.hunger > 55.0 {
        let diner = w.map.first_of(BuildingKind::Diner).filter(|d| w.map.buildings[*d].open(hour));
        let store = w.map.first_of(BuildingKind::Store).filter(|s| w.map.buildings[*s].open(hour));
        let r: f32 = w.rng.gen();
        if p.bed.is_some() && p.money >= 3 && (r < 0.55 || diner.is_none()) {
            set_intent(w, i, IntentKind::EatHome, p.bed.unwrap(), p.home);
            return;
        }
        if let (Some(d), true) = (diner, p.money >= 9) {
            let seats = w.map.buildings[d].seats.clone();
            if let Some(s) = free_spot(w, i, &seats) {
                set_intent(w, i, IntentKind::Eat, s, Some(d));
                return;
            }
        }
        if let Some(s) = store {
            let svc = w.map.buildings[s].service.clone();
            if let Some(t) = svc.first() {
                if p.money >= 5 {
                    set_intent(w, i, IntentKind::Shop, *t, Some(s));
                    return;
                } else if p.hunger > 80.0 && (p.has(Trait::Greedy) || p.money <= 0) {
                    set_intent(w, i, IntentKind::Steal, *t, Some(s));
                    return;
                }
            }
        }
        if let Some(b) = p.bed {
            set_intent(w, i, IntentKind::EatHome, b, p.home);
            return;
        }
        if p.hunger > 70.0 {
            // No money, no home: the bins, then.
            let bin = (0..w.map.tiles.len())
                .filter(|k| w.map.tiles[*k].furn == Furniture::Trash)
                .map(|k| Pos::new(k as i32 % w.map.w, k as i32 / w.map.w))
                .min_by_key(|q| q.dist(pos));
            if let Some(b) = bin {
                set_intent(w, i, IntentKind::Scavenge, b, None);
                return;
            }
        }
    }

    // Hurt, but not dying: the clinic, if it's been a while since the last wasted trip.
    if p.body.health_frac() < 0.6 && now - p.last_clinic > 360 {
        if go_clinic(w, i) {
            return;
        }
    }

    // Drink.
    let bar = w.map.first_of(BuildingKind::Bar).filter(|b| w.map.buildings[*b].open(hour));
    if let Some(b) = bar {
        let wants = p.stress > 55.0 || (p.has(Trait::Drunkard) && (hour >= 17 || hour < 2)) || (p.lonely > 60.0 && hour >= 19);
        if wants && p.money >= 6 {
            let seats = w.map.buildings[b].seats.clone();
            if let Some(s) = free_spot(w, i, &seats) {
                set_intent(w, i, IntentKind::Drink, s, Some(b));
                return;
            }
        }
    }

    // Smoke.
    if p.has(Trait::Smoker) && w.chance(0.03) {
        social::smoke(w, i);
    }

    // Company.
    if p.lonely > 40.0 {
        let place = if (17..23).contains(&hour) { bar } else { None }
            .or_else(|| if (7..21).contains(&hour) { w.map.first_of(BuildingKind::Park) } else { None })
            .or(p.home);
        if let Some(b) = place {
            let seats = w.map.buildings[b].seats.clone();
            if let Some(s) = free_spot(w, i, &seats) {
                set_intent(w, i, IntentKind::Socialize, s, Some(b));
                return;
            }
        }
    }

    // Nothing to do. The city is good at providing nothing to do.
    if p.bed.is_some() && (w.chance(0.4) || w.is_night()) {
        set_intent(w, i, IntentKind::Loiter, p.bed.unwrap(), p.home);
    } else if let Some(t) = random_nearby_walkable(w, pos, 12) {
        set_intent(w, i, IntentKind::Wander, t, None);
    }
    let _ = now;
}

fn execute(w: &mut World, i: usize) {
    let now = w.turn;
    let it = w.people[i].intent;
    let pos = w.people[i].pos;
    match it.kind {
        IntentKind::None => {}
        IntentKind::Fight(t) => {
            let tp = w.people[t].clone();
            if !tp.alive || tp.jailed_until > now || w.people[i].hostile_to != Some(t) && w.people[i].hostile_until < now {
                clear(w, i);
                w.people[i].hostile_to = None;
                return;
            }
            if tp.unconscious_until > now {
                let hate = w.people[i].opinion(t);
                if !(hate <= -85 && w.chance(0.5)) {
                    let (a, b) = (w.people[i].name.clone(), w.name(t));
                    w.say(Some(pos), &[i, t], LogKind::Bad, format!("{} stands over {}, breathing hard, then walks away.", a, b));
                    clear(w, i);
                    w.people[i].hostile_to = None;
                    return;
                }
            }
            if pos.adjacent(tp.pos) {
                combat::attack(w, i, t);
            } else if pos.dist(tp.pos) > 14 {
                clear(w, i);
                w.people[i].hostile_to = None;
            } else {
                step_to(w, i, tp.pos, true);
            }
        }
        IntentKind::Chase(t) => {
            let tp = w.people[t].clone();
            if !tp.alive || tp.jailed_until > now || w.heat(t) == 0 {
                clear(w, i);
                return;
            }
            if pos.adjacent(tp.pos) {
                if tp.hostile_to == Some(i) && tp.unconscious_until <= now {
                    combat::attack(w, i, t);
                } else {
                    combat::arrest(w, i, t);
                }
            } else if !step_to(w, i, tp.pos, true) {
                clear(w, i);
            }
        }
        IntentKind::Extinguish => {
            let Some(fp) = fire::nearest_burning(w, pos).filter(|f| f.dist(pos) < 40) else {
                clear(w, i);
                return;
            };
            if pos.dist(fp) <= 1 {
                fire::extinguish(w, fp);
                if w.chance(0.2) {
                    let n = w.people[i].name.clone();
                    w.say(Some(pos), &[i], LogKind::Good, format!("{} hoses down the flames.", n));
                }
            } else if !step_to(w, i, fp, true) {
                clear(w, i);
            }
        }
        IntentKind::Flee => {
            if pos == it.target || now > it.until || !step_to(w, i, it.target, false) {
                clear(w, i);
            }
        }
        IntentKind::Arson => {
            if pos.dist(it.target) <= 1 {
                fire::ignite(w, it.target);
                w.people[i].hum = false;
                let n = w.people[i].name.clone();
                let f = w.map.get(it.target).furn.name();
                let f = if f.is_empty() { "floor" } else { f };
                w.say(Some(pos), &[i], LogKind::Weird, format!("{} sets fire to the {} with a look of profound relief.", n, f));
                w.crime(crate::world::CrimeKind::Arson, i, None, it.target);
                clear(w, i);
            } else if !step_to(w, i, it.target, false) {
                w.people[i].hum = false;
                clear(w, i);
            }
        }
        _ => {
            if !it.started {
                if pos == it.target || (!w.map.walkable(it.target) && pos.adjacent(it.target)) {
                    w.people[i].intent.started = true;
                    on_arrive(w, i);
                } else if !step_to(w, i, it.target, false) {
                    clear(w, i);
                }
            } else {
                activity(w, i);
            }
        }
    }
}

fn pay(w: &mut World, i: usize, b: Option<usize>, amount: i32) -> bool {
    if w.people[i].money < amount {
        return false;
    }
    w.people[i].money -= amount;
    if let Some(b) = b {
        w.map.buildings[b].till += amount;
    }
    true
}

fn on_arrive(w: &mut World, i: usize) {
    let now = w.turn;
    let it = w.people[i].intent;
    let pos = w.people[i].pos;
    match it.kind {
        IntentKind::Sleep => {
            w.people[i].sleeping = true;
        }
        IntentKind::Eat => {
            if pay(w, i, it.building, 9) {
                w.people[i].intent.until = now + 25;
            } else {
                clear(w, i);
            }
        }
        IntentKind::EatHome => {
            let _ = pay(w, i, None, 3);
            w.people[i].intent.until = now + 20;
        }
        IntentKind::Scavenge => {
            let p = &mut w.people[i];
            p.hunger = (p.hunger - 35.0).max(0.0);
            p.stress = (p.stress + 5.0).min(100.0);
            let n = p.name.clone();
            if w.chance(0.4) {
                w.say(Some(pos), &[i], LogKind::Bad, format!("{} eats something out of a trash can. It was probably food once.", n));
            }
            clear(w, i);
        }
        IntentKind::Drink => {
            if pay(w, i, it.building, 6) {
                let p = &mut w.people[i];
                p.drunk += if p.has(Trait::Drunkard) { 28.0 } else { 20.0 };
                p.stress = (p.stress - 15.0).max(0.0);
                p.intent.until = now + 45;
            } else {
                clear(w, i);
            }
        }
        IntentKind::Socialize | IntentKind::Loiter | IntentKind::Wander => {
            let d = w.rng.gen_range(30..100);
            w.people[i].intent.until = now + d;
        }
        IntentKind::Clinic => {
            w.people[i].intent.until = now + 90;
        }
        IntentKind::Shop => {
            let p = w.people[i].clone();
            let item = if p.hunger > 40.0 {
                ItemKind::Snack
            } else if p.has(Trait::Smoker) && !p.inventory.contains(&ItemKind::Cigarettes) {
                ItemKind::Cigarettes
            } else if p.stress > 60.0 && w.chance(0.3) {
                ItemKind::Lottery
            } else {
                ItemKind::Beer
            };
            if pay(w, i, it.building, item.price()) {
                if item == ItemKind::Snack {
                    let q = &mut w.people[i];
                    q.hunger = (q.hunger - 40.0).max(0.0);
                } else if item == ItemKind::Lottery {
                    if w.chance(0.03) {
                        w.people[i].money += 250;
                        let n = w.people[i].name.clone();
                        w.say(Some(pos), &[i], LogKind::Good, format!("{} wins $250 on a scratch-off and makes a noise like a kettle.", n));
                    }
                } else {
                    w.people[i].inventory.push(item);
                }
            }
            clear(w, i);
        }
        IntentKind::Steal => {
            if let Some(b) = it.building {
                let take = w.map.buildings[b].till.min(w.rng.gen_range(10..60)).max(0);
                w.map.buildings[b].till -= take;
                let p = &mut w.people[i];
                p.money += take;
                p.hunger = (p.hunger - 30.0).max(0.0);
                let n = p.name.clone();
                w.say(Some(pos), &[i], LogKind::Bad, format!("{} pockets something from the shelves and ${} from the till.", n, take));
                w.crime(crate::world::CrimeKind::Theft, i, None, pos);
            }
            clear(w, i);
        }
        IntentKind::Work => {
            if w.people[i].is_player {
                w.say(None, &[i], LogKind::Normal, "You clock in.");
            }
        }
        _ => {}
    }
}

fn activity(w: &mut World, i: usize) {
    let now = w.turn;
    let it = w.people[i].intent;
    let pos = w.people[i].pos;
    match it.kind {
        IntentKind::Work => {
            if !w.on_shift(i) {
                clear(w, i);
                return;
            }
            let role = w.role(i);
            match role {
                Role::Janitor | Role::Worker if w.chance(0.04) => {
                    if let (Some(b), Some(t)) = (it.building, random_nearby_walkable(w, pos, 3)) {
                        if w.map.buildings[b].contains(t) {
                            w.people[i].pos = t;
                            w.people[i].intent.target = t;
                        }
                    }
                }
                Role::Cook => {
                    let p = &w.people[i];
                    let risk = 0.0002 * (1.0 + p.drunk / 25.0 + if p.energy < 30.0 { 1.5 } else { 0.0 });
                    if w.rng.gen::<f32>() < risk {
                        let stove = DIRS8.iter().map(|(dx, dy)| pos.add(*dx, *dy)).find(|q| w.map.get(*q).furn == Furniture::Stove);
                        if let Some(s) = stove {
                            if fire::ignite(w, s) {
                                let n = w.people[i].name.clone();
                                w.say(Some(pos), &[i], LogKind::Danger, format!("Grease fire! {} stares at the stove as it goes up.", n));
                            }
                        }
                    }
                }
                Role::Medic => {
                    // Treat whoever is waiting.
                    if now % 6 == 0 {
                        let b = it.building;
                        let patients: Vec<usize> = w
                            .people
                            .iter()
                            .filter(|q| {
                                q.alive
                                    && (q.intent.kind == IntentKind::Clinic && q.intent.started && q.intent.building == b
                                        || q.is_player && b.map(|b| w.map.buildings[b].contains(q.pos)).unwrap_or(false) && q.body.hurt())
                            })
                            .map(|q| q.id)
                            .collect();
                        for pt in patients {
                            let q = &mut w.people[pt];
                            q.body.bleeding = 0;
                            if let Some(k) = (0..6).min_by_key(|k| q.body.hp[*k] * 100 / q.body.max[*k]) {
                                if q.body.hp[k] < q.body.max[k] {
                                    q.body.hp[k] += 1;
                                }
                            }
                            q.body.blood = (q.body.blood + 3.0).min(100.0);
                        }
                    }
                }
                _ => {}
            }
            social::maybe_chat(w, i, 0.01);
        }
        IntentKind::Sleep => {
            let p = &w.people[i];
            if (p.energy >= 97.0 && !sleep_window(w, i)) || p.energy >= 100.0 || (p.job.is_some() && w.on_shift_at(i, now + 40) && p.energy > 40.0) {
                clear(w, i);
            } else {
                w.people[i].sleeping = true;
            }
        }
        IntentKind::Eat | IntentKind::EatHome => {
            social::maybe_chat(w, i, 0.03);
            if now >= it.until {
                let p = &mut w.people[i];
                p.hunger = (p.hunger - 70.0).max(0.0);
                clear(w, i);
            }
        }
        IntentKind::Drink => {
            social::maybe_chat(w, i, 0.06);
            if now >= it.until {
                let again = {
                    let p = &w.people[i];
                    p.money >= 6 && (p.has(Trait::Drunkard) || p.stress > 50.0) && p.drunk < 90.0
                };
                if again && w.chance(0.5) {
                    on_arrive(w, i);
                } else {
                    clear(w, i);
                }
            }
        }
        IntentKind::Socialize | IntentKind::Loiter | IntentKind::Wander => {
            social::maybe_chat(w, i, if it.kind == IntentKind::Socialize { 0.08 } else { 0.02 });
            if now >= it.until {
                clear(w, i);
            }
        }
        IntentKind::Clinic => {
            let p = &w.people[i];
            if p.body.health_frac() > 0.95 && p.body.bleeding == 0 {
                w.people[i].money -= 60;
                if w.people[i].is_player {
                    w.say(None, &[i], LogKind::Bad, "You're patched up. The bill is $60. They take it whether you have it or not.");
                }
                clear(w, i);
            } else if now >= it.until || p.hunger > 85.0 {
                w.people[i].last_clinic = now;
                clear(w, i);
            }
        }
        _ => clear(w, i),
    }
}
