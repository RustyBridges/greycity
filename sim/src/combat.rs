//! Violence, arrests and jail.

use crate::map::BuildingKind;
use crate::person::*;
use crate::world::{Busy, CrimeKind, LogKind, Role, World};
use rand::Rng;

pub fn attack(w: &mut World, a: usize, d: usize) {
    let now = w.turn;
    if !w.people[d].alive || a == d {
        return;
    }
    let pid = w.player;
    let ap = w.people[a].clone();
    let dp = w.people[d].clone();
    let pos = dp.pos;
    // Was this self-defence or police work? Then it's not a crime.
    let defensive = dp.hostile_to == Some(a) && dp.hostile_until > now;
    let police = w.role(a) == Role::Cop && w.on_shift(a) && w.heat(d) > 0;
    let down = dp.unconscious_until > now || dp.sleeping;

    let hit_chance = if down { 0.98 } else { 0.72 - ap.drunk * 0.004 + if ap.has(Trait::Brave) { 0.05 } else { 0.0 } };
    let knife = ap.weapon();
    let an = w.cname(a);
    let dn = w.name(d);
    let s = if a == pid { "" } else { "s" };

    if w.rng.gen::<f32>() > hit_chance {
        let lines = ["swing{s} at {d} and miss{es}", "lunge{s} at {d} and stumble{s}", "throw{s} a wild punch at {d}, hitting only air"];
        let l = lines[w.rng.gen_range(0..lines.len())]
            .replace("{s}", s)
            .replace("{es}", if a == pid { "" } else { "es" })
            .replace("{d}", &dn);
        w.say(Some(pos), &[a, d], LogKind::Bad, format!("{} {}.", an, l));
    } else {
        let roll = w.rng.gen_range(0..100);
        let part = match roll {
            0..=14 => HEAD,
            15..=49 => TORSO,
            50..=62 => 2,
            63..=75 => 3,
            76..=87 => 4,
            _ => 5,
        };
        let mut dmg = w.rng.gen_range(2..=5);
        if ap.has(Trait::Hothead) {
            dmg += 1;
        }
        if knife {
            dmg += 4;
        }
        if ap.drunk > 50.0 {
            dmg += 1;
        }
        let verb = if knife {
            if a == pid { "stab" } else { "stabs" }
        } else {
            match w.rng.gen_range(0..4) {
                0 => if a == pid { "kick" } else { "kicks" },
                1 => if a == pid { "elbow" } else { "elbows" },
                _ => if a == pid { "punch" } else { "punches" },
            }
        };
        let possessive = if d == pid { "your".to_string() } else { format!("{}'s", w.people[d].first_name()) };
        let tgt = if d == pid { "you".to_string() } else { w.people[d].name.clone() };
        let _ = tgt;
        {
            let q = &mut w.people[d];
            // Fists rarely kill outright: you go down first. Knives, and kicking
            // someone who is already down, are another matter.
            if !knife && !down && (part == HEAD || part == TORSO) && q.body.hp[part] - dmg <= 0 {
                dmg = (q.body.hp[part] - 1).max(0);
                q.unconscious_until = now + 30;
            }
            q.body.hp[part] -= dmg;
            if knife {
                q.body.bleeding += w.rng.gen_range(1..=2);
            } else if w.rng.gen::<f32>() < 0.12 {
                q.body.bleeding += 1;
            }
            q.stress = (q.stress + 10.0).min(100.0);
            q.sleeping = false;
        }
        w.say(Some(pos), &[a, d], LogKind::Danger, format!("{} {} {} {}.", an, verb, possessive, PART_NAMES[part]));
        let q = &w.people[d];
        let head_frac = q.body.hp[HEAD] as f32 / q.body.max[HEAD] as f32;
        let ko_chance = if part == HEAD && !knife { 0.2 + (1.0 - head_frac) * 0.6 } else { 0.0 } + if police { 0.1 } else { 0.0 };
        if q.body.hp[HEAD] <= 0 || q.body.hp[TORSO] <= 0 {
            let cause = if knife { format!("stabbed by {}", w.people[a].name) } else { format!("beaten to death by {}", w.people[a].name) };
            let killer = if defensive || police { None } else { Some(a) };
            if !defensive && !police {
                // Murder is recorded by kill(); assault is implied.
            }
            w.kill(d, cause, killer);
            if killer.is_none() {
                w.people[a].kills += 1;
            }
        } else {
            if q.body.hp[part] <= 0 && part > TORSO {
                w.say(Some(pos), &[d], LogKind::Danger, format!("{} {} is ruined.", capitalise(&possessive), PART_NAMES[part]));
            }
            if w.rng.gen::<f32>() < ko_chance {
                let t = w.rng.gen_range(20..90);
                w.people[d].unconscious_until = now + t;
                w.people[d].intent = Intent::default();
                let dn2 = w.cname(d);
                w.say(Some(pos), &[d], LogKind::Danger, format!("{} go{} down hard.", dn2, if d == pid { "" } else { "es" }));
            }
        }
    }

    // Consequences.
    {
        let q = &mut w.people[d];
        q.adjust_opinion(a, -25);
        q.remember(now, MemKind::Attacked, Some(a), Some(d));
        if q.alive && q.unconscious_until <= now && !(q.is_player && !w.autopilot) {
            if !q.has(Trait::Coward) || q.hostile_to.is_some() {
                q.hostile_to = Some(a);
                q.hostile_until = now + 40;
            }
        } else if q.is_player {
            q.hostile_to = Some(a);
            q.hostile_until = now + 40;
        }
    }
    w.people[a].hostile_to = Some(d);
    w.people[a].hostile_until = now + 40;
    if d == pid {
        w.interrupt = true;
    }
    if !defensive && !police && w.people[d].alive {
        w.crime(CrimeKind::Assault, a, Some(d), pos);
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn arrest(w: &mut World, cop: usize, perp: usize) {
    let now = w.turn;
    let pid = w.player;
    let p = w.people[perp].clone();
    let resists = perp != pid && p.unconscious_until <= now && (p.has(Trait::Hothead) || p.drunk > 40.0) && w.rng.gen::<f32>() < 0.35;
    let (cn, pn) = (w.people[cop].name.clone(), w.name(perp));
    if resists {
        w.people[perp].hostile_to = Some(cop);
        w.people[perp].hostile_until = now + 40;
        w.say(Some(p.pos), &[perp], LogKind::Danger, format!("{} resists arrest!", pn));
        return;
    }
    let sev: i32 = w
        .crimes
        .iter()
        .filter(|c| c.perp == perp && !c.resolved && c.reported)
        .map(|c| c.kind.severity())
        .sum();
    let hours = (sev * 4).clamp(4, 96) as u64;
    for c in w.crimes.iter_mut().filter(|c| c.perp == perp && !c.resolved) {
        c.resolved = true;
    }
    let cell = w
        .map
        .first_of(BuildingKind::CivicStation)
        .and_then(|b| w.map.buildings[b].cells.get(w.rng.gen_range(0..3)).copied());
    let fine = sev * 30;
    {
        let q = &mut w.people[perp];
        q.jailed_until = now + hours * 60;
        q.record += 1;
        q.money -= fine;
        q.intent = Intent::default();
        q.path.clear();
        q.hostile_to = None;
        q.sleeping = false;
        q.remember(now, MemKind::Arrested, Some(cop), None);
        if let Some(c) = cell {
            q.pos = c;
        }
    }
    w.people[cop].intent = Intent::default();
    w.people[cop].hostile_to = None;
    w.people[cop].performance += 1;
    w.stats.arrests += 1;
    if perp == pid {
        w.busy = Some(Busy::Jail);
        w.say(
            None,
            &[pid],
            LogKind::Danger,
            format!(
                "Officer {} arrests you. You are booked, photographed and fined ${}, then left in a cell that smells like other people's decisions. ({} hours)",
                cn, fine, hours
            ),
        );
    } else {
        w.say(Some(p.pos), &[cop, perp], LogKind::Info, format!("Officer {} arrests {}.", cn, pn));
    }
}

pub fn release(w: &mut World, i: usize) {
    let out = w
        .map
        .first_of(BuildingKind::CivicStation)
        .and_then(|b| w.map.buildings[b].entrances.first().copied())
        .map(|e| e.add(0, 1));
    let p = &mut w.people[i];
    p.jailed_until = 0;
    p.stress = (p.stress + 15.0).min(100.0);
    if let Some(o) = out {
        p.pos = o;
    }
    if i == w.player {
        w.say(None, &[i], LogKind::Info, "You are released. The city has not noticed you were gone.");
    } else {
        let n = w.people[i].name.clone();
        w.say(out, &[i], LogKind::Info, format!("{} is released from the precinct, squinting.", n));
    }
}
