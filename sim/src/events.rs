//! Things that happen *to* the city: weird events, the payphone, and the
//! offers that drag the player into being the bad guy.

use crate::fire;
use crate::map::BuildingKind;
use crate::path;
use crate::person::*;
use crate::world::*;
use rand::seq::SliceRandom;
use rand::Rng;

pub fn hourly(w: &mut World) {
    if w.chance(0.12) {
        weird_event(w);
    }
    payphone(w);
    // Someone nearby with a grudge might come to you with a proposal.
    if w.offer.is_none() && w.chance(0.2) && w.people[w.player].alive {
        let pp = w.people[w.player].pos;
        let now = w.turn;
        let near: Vec<usize> = w
            .people
            .iter()
            .filter(|q| q.id != w.player && q.awake(now) && q.jailed_until <= now && q.pos.dist(pp) <= 7)
            .filter(|q| w.visible[w.map.idx(q.pos)])
            .map(|q| q.id)
            .collect();
        if let Some(&n) = near.choose(&mut w.rng) {
            maybe_offer(w, n, false);
        }
    }
}

fn fill(w: &mut World, text: &str, person: Option<usize>, building: Option<usize>) -> String {
    let mut s = text.to_string();
    if let Some(p) = person {
        s = s.replace("{person}", &w.people[p].name);
    }
    if let Some(b) = building {
        s = s.replace("{building}", &w.map.buildings[b].name);
    }
    s
}

fn random_resident(w: &mut World, prefer_weird: bool) -> Option<usize> {
    let pid = w.player;
    let alive: Vec<usize> = w.people.iter().filter(|p| p.alive && p.id != pid).map(|p| p.id).collect();
    if prefer_weird {
        let weird: Vec<usize> = alive
            .iter()
            .copied()
            .filter(|i| w.people[*i].has(Trait::Weird) || w.people[*i].stress > 70.0)
            .filter(|i| w.people[*i].jailed_until <= w.turn)
            .collect();
        if let Some(x) = weird.choose(&mut w.rng) {
            return Some(*x);
        }
    }
    alive.choose(&mut w.rng).copied()
}

pub fn weird_event(w: &mut World) {
    let total: u32 = w.weird.iter().map(|e| e.weight).sum();
    let mut roll = w.rng.gen_range(0..total.max(1));
    let mut pick = None;
    for e in &w.weird {
        if roll < e.weight {
            pick = Some(e.clone());
            break;
        }
        roll -= e.weight;
    }
    let Some(ev) = pick else { return };
    let pid = w.player;
    let b = w.rng.gen_range(0..w.map.buildings.len());
    match ev.effect {
        WeirdEffect::Message => {
            let p = random_resident(w, false);
            let s = fill(w, &ev.text, p, Some(b));
            w.say(None, &[], LogKind::Weird, s);
        }
        WeirdEffect::StressAll(n) => {
            for p in w.people.iter_mut() {
                p.stress = (p.stress + n as f32).clamp(0.0, 100.0);
            }
            let s = fill(w, &ev.text, None, Some(b));
            w.say(None, &[], LogKind::Weird, s);
        }
        WeirdEffect::StressPlayer(n) => {
            let p = &mut w.people[pid];
            p.stress = (p.stress + n as f32).clamp(0.0, 100.0);
            let s = fill(w, &ev.text, None, Some(b));
            w.say(None, &[pid], LogKind::Weird, s);
        }
        WeirdEffect::HumTouch => {
            if let Some(x) = random_resident(w, true) {
                w.people[x].hum = true;
                w.people[x].intent = Intent::default();
                let pos = w.people[x].pos;
                let s = fill(w, &ev.text, Some(x), None);
                w.say(Some(pos), &[x], LogKind::Weird, s);
            }
        }
        WeirdEffect::ElectricalFire => {
            let b = w.map.buildings.iter().filter(|b| b.kind != BuildingKind::Park).map(|b| b.id).collect::<Vec<_>>();
            if let Some(&b) = b.choose(&mut w.rng) {
                if let Some(t) = fire::random_fuel_in(w, b) {
                    fire::ignite(w, t);
                    let s = fill(w, &ev.text, None, Some(b));
                    w.say(None, &[], LogKind::Weird, s);
                }
            }
        }
        WeirdEffect::Windfall(n) => {
            if let Some(x) = random_resident(w, false) {
                w.people[x].money += n;
                for q in w.people.iter_mut() {
                    if q.has(Trait::Greedy) {
                        q.adjust_opinion(x, -10);
                    }
                }
                let s = fill(w, &ev.text, Some(x), None);
                w.say(None, &[], LogKind::Weird, s);
            }
        }
        WeirdEffect::Doppelganger => {
            if let Some(x) = random_resident(w, false) {
                w.people[pid].stress = (w.people[pid].stress + 6.0).min(100.0);
                let s = fill(w, &ev.text, Some(x), None);
                w.say(None, &[pid], LogKind::Weird, s);
            }
        }
    }
}

fn payphone(w: &mut World) {
    let h = w.hour();
    if w.phone_ringing.is_some() || w.offer.is_some() || !(h >= 22 || h < 5) {
        return;
    }
    let pid = w.player;
    let p = &w.people[pid];
    if !p.awake(w.turn) || p.jailed_until > w.turn {
        return;
    }
    let low = p.stress > 45.0
        || p.money < 0
        || p.home.is_none()
        || p.memories.iter().any(|m| m.kind == MemKind::SawCorpse || m.kind == MemKind::SawMurder);
    let pos = p.pos;
    let phone = w.map.payphones.iter().copied().filter(|ph| ph.dist(pos) <= 25).min_by_key(|ph| ph.dist(pos));
    if let Some(ph) = phone {
        if low && w.chance(0.3) {
            w.phone_ringing = Some((ph, w.turn + 20));
            w.say(None, &[pid], LogKind::Offer, "Somewhere close, a payphone is ringing.");
        }
    }
}

pub fn answer_phone(w: &mut World) {
    let pid = w.player;
    if w.phone_ringing.is_none() {
        let lines = ["Dial tone. There is no one you could call.", "You lift the receiver. The dial tone sounds tired.", "The phone eats your quarter and gives you nothing. Relatable."];
        let l = lines.choose(&mut w.rng).unwrap().to_string();
        w.say(None, &[pid], LogKind::Normal, l);
        return;
    }
    w.phone_ringing = None;
    let now = w.turn;
    let roll = w.rng.gen_range(0..3);
    let (kind, reward, speech, summary) = match roll {
        0 => (
            ObjKind::Fires(3),
            300,
            "A voice like a radiator knocking: \"The city is too quiet. Make it bright. Three fires. You have until the end of the week.\"".to_string(),
            "Start three fires (the voice on the payphone)".to_string(),
        ),
        1 => {
            let Some(x) = random_resident(w, false) else { return };
            let n = w.people[x].name.clone();
            (
                ObjKind::Hurt(x),
                400,
                format!("A child's voice, very calm: \"There is a person named {}. They are going to be very important. Make sure they aren't.\"", n),
                format!("Hurt {} badly (the voice on the payphone)", n),
            )
        }
        _ => {
            let ids: Vec<usize> = w.map.buildings.iter().filter(|b| b.kind != BuildingKind::Park).map(|b| b.id).collect();
            let b = *ids.choose(&mut w.rng).unwrap();
            let n = w.map.buildings[b].name.clone();
            (
                ObjKind::Burn(b),
                500,
                format!("Your own voice, slightly delayed: \"{} has been standing for too long. It is tired. Help it lie down.\"", n),
                format!("Burn down part of {} (the voice on the payphone)", n),
            )
        }
    };
    let baseline = baseline_for(w, kind);
    let obj = Objective {
        source: OfferSource::Payphone,
        kind,
        reward,
        deadline: now + 5 * MINUTES_PER_DAY,
        baseline,
        summary,
        done: false,
        failed: false,
    };
    w.say(None, &[pid], LogKind::Weird, "You pick up the receiver. It's warm.");
    w.offer = Some((obj, speech));
}

fn baseline_for(w: &World, kind: ObjKind) -> u32 {
    match kind {
        ObjKind::Burn(b) => w.map.buildings[b].burned,
        ObjKind::Fires(_) => w.stats.player_fires,
        ObjKind::Rob(_) => w.stats.player_robberies,
        _ => 0,
    }
}

/// An NPC with a grudge, or a desperate scheme, might propose something.
pub fn maybe_offer(w: &mut World, n: usize, player_initiated: bool) {
    if w.offer.is_some() || w.objectives.iter().filter(|o| !o.done && !o.failed).count() >= 2 {
        return;
    }
    let pid = w.player;
    let now = w.turn;
    let p = w.people[n].clone();
    let pl = w.people[pid].clone();
    if !p.awake(now) {
        return;
    }
    let desperate = pl.money < 40 || pl.home.is_none() || pl.job.is_none() || pl.rent_owed > 0;
    let likes = p.opinion(pid);
    if likes < -10 || (likes < 10 && !desperate) {
        return;
    }
    let grudge = p
        .opinions
        .iter()
        .filter(|(x, o)| **o <= -45 && **x != pid && w.people[**x].alive && w.people[**x].jailed_until <= now)
        .min_by_key(|(_, o)| **o)
        .map(|(x, o)| (*x, *o));

    let chance = if player_initiated { 0.45 } else { 0.6 };
    if let Some((x, hate)) = grudge {
        if !w.chance(chance) {
            return;
        }
        let xn = w.people[x].name.clone();
        let opener = if player_initiated { format!("{} lowers their voice.", p.name) } else { format!("{} sidles up to you, checking over their shoulder.", p.name) };
        let target_building = w.people[x].workplace.or(w.people[x].home);
        let (kind, reward, speech, summary) = if hate <= -80 && w.chance(0.6) {
            let r = w.rng.gen_range(5..10) * 100;
            (
                ObjKind::Kill(x),
                r,
                format!("{} \"{}. You know them? I want them gone. Not hurt. Gone. ${}. I'll know when it's done.\"", opener, xn, r),
                format!("Kill {} (for {})", xn, p.name),
            )
        } else if (p.has(Trait::Greedy) || p.has(Trait::Petty)) && target_building.is_some() && w.chance(0.5) {
            let b = target_building.unwrap();
            let bn = w.map.buildings[b].name.clone();
            let r = w.rng.gen_range(3..6) * 100;
            (
                ObjKind::Burn(b),
                r,
                format!("{} \"{} spends every day at {}. Wouldn't it be a shame if it caught fire? ${}, and I was never here.\"", opener, xn, bn, r),
                format!("Burn {} (for {}, to spite {})", bn, p.name, xn),
            )
        } else {
            let r = w.rng.gen_range(15..30) * 10;
            (
                ObjKind::Hurt(x),
                r,
                format!("{} \"You know {}? I'd pay ${} to see them hurt. Not dead. Just... reconsidering things.\"", opener, xn, r),
                format!("Hurt {} badly (for {})", xn, p.name),
            )
        };
        let obj = Objective {
            source: OfferSource::Person(n),
            kind,
            reward,
            deadline: now + 3 * MINUTES_PER_DAY,
            baseline: baseline_for(w, kind),
            summary,
            done: false,
            failed: false,
        };
        w.offer = Some((obj, speech));
        w.interrupt = true;
    } else if desperate && p.has(Trait::Greedy) && w.chance(chance * 0.6) {
        let Some(store) = w.map.first_of(BuildingKind::Store) else { return };
        let bn = w.map.buildings[store].name.clone();
        let speech = format!(
            "{} looks you up and down. \"You look like you need money. The register at {} has a few hundred in it most nights. Grab it, and I'll throw in $100 for the entertainment.\"",
            p.name, bn
        );
        let kind = ObjKind::Rob(store);
        let obj = Objective {
            source: OfferSource::Person(n),
            kind,
            reward: 100,
            deadline: now + 3 * MINUTES_PER_DAY,
            baseline: baseline_for(w, kind),
            summary: format!("Rob the till at {} (for {})", bn, p.name),
            done: false,
            failed: false,
        };
        w.offer = Some((obj, speech));
        w.interrupt = true;
    }
}

pub fn accept_offer(w: &mut World) {
    let Some((obj, _)) = w.offer.take() else { return };
    let pid = w.player;
    if let OfferSource::Person(n) = obj.source {
        w.people[n].adjust_opinion(pid, 15);
        let nn = w.people[n].name.clone();
        w.say(None, &[pid], LogKind::Offer, format!("You nod. {} doesn't smile, exactly. \"Three days.\"", nn));
    } else {
        w.say(None, &[pid], LogKind::Weird, "You say yes. The line was dead the whole time. You hang up anyway.");
    }
    w.objectives.push(obj);
}

pub fn decline_offer(w: &mut World) {
    let Some((obj, _)) = w.offer.take() else { return };
    let pid = w.player;
    if let OfferSource::Person(n) = obj.source {
        w.people[n].adjust_opinion(pid, -8);
        let nn = w.people[n].name.clone();
        w.say(None, &[pid], LogKind::Normal, format!("{} shrugs. \"Forget I said anything.\" You won't.", nn));
    } else {
        w.say(None, &[pid], LogKind::Weird, "You hang up. The phone keeps ringing for a while after, very quietly.");
    }
}

pub fn check_objectives(w: &mut World) {
    if w.objectives.is_empty() {
        return;
    }
    let now = w.turn;
    let pid = w.player;
    for k in 0..w.objectives.len() {
        let o = w.objectives[k].clone();
        if o.done || o.failed {
            continue;
        }
        let done = match o.kind {
            ObjKind::Hurt(x) => {
                let p = &w.people[x];
                !p.alive || p.unconscious_until > now || p.body.health_frac() < 0.5
            }
            ObjKind::Kill(x) => !w.people[x].alive,
            ObjKind::Burn(b) => w.map.buildings[b].burned >= o.baseline + 12,
            ObjKind::Rob(_) => w.stats.player_robberies > o.baseline,
            ObjKind::Fires(n) => w.stats.player_fires >= o.baseline + n,
        };
        if done {
            w.objectives[k].done = true;
            match o.source {
                OfferSource::Person(g) if w.people[g].alive => {
                    w.people[g].money -= o.reward;
                    w.people[pid].money += o.reward;
                    w.people[g].adjust_opinion(pid, 20);
                    let gn = w.people[g].name.clone();
                    w.say(None, &[pid], LogKind::Offer, format!("Done. Later, an envelope with ${} turns up in your pocket. You never saw {} put it there.", o.reward, gn));
                }
                OfferSource::Person(g) => {
                    let gn = w.people[g].name.clone();
                    w.say(None, &[pid], LogKind::Offer, format!("Done. But {} is dead, so nobody is paying you. You did it for free, apparently.", gn));
                }
                OfferSource::Payphone => {
                    w.people[pid].money += o.reward;
                    w.people[pid].stress = (w.people[pid].stress - 40.0).max(0.0);
                    w.say(
                        None,
                        &[pid],
                        LogKind::Weird,
                        format!("Your pocket is heavier: ${} in old bills, damp, smelling faintly of the sea. The hum is quieter for a while.", o.reward),
                    );
                }
            }
        } else if now > o.deadline {
            w.objectives[k].failed = true;
            match o.source {
                OfferSource::Person(g) => {
                    w.people[g].adjust_opinion(pid, -30);
                    let gn = w.people[g].name.clone();
                    w.say(None, &[pid], LogKind::Bad, format!("You let the deadline pass. {} has stopped returning your looks.", gn));
                }
                OfferSource::Payphone => {
                    w.people[pid].stress = (w.people[pid].stress + 25.0).min(100.0);
                    w.say(None, &[pid], LogKind::Weird, "The deadline passes. Every payphone you walk past rings once, and stops.");
                }
            }
        }
    }
    let _ = path::los;
}
