//! Talking, insulting, gossiping, smoking. Where most grudges are born.

use crate::fire;
use crate::map::DIRS8;
use crate::person::*;
use crate::world::{LogKind, World};
use rand::seq::SliceRandom;
use rand::Rng;

const CHATS: [&str; 16] = [
    "{a} and {b} agree that things used to be better. Neither can say when.",
    "{a} tells {b} about a dream where the rent was cheaper.",
    "{a} shows {b} a photo of a dog. It is not {a}'s dog.",
    "{a} and {b} discuss the weather. It is grey.",
    "{a} complains to {b} about their back. {b} complains about theirs. It becomes competitive.",
    "{a} asks {b} if they've heard the hum. {b} changes the subject.",
    "{a} laughs at something {b} said. {b} wasn't joking.",
    "{a} and {b} talk about quitting their jobs. Neither will.",
    "{a} explains a get-rich-quick scheme to {b}. {b} asks good questions, which is worse.",
    "{a} and {b} share a long silence. It is the best part of the conversation.",
    "{a} tells {b} they look tired. {b} says thank you, for some reason.",
    "{a} and {b} argue about whether pigeons have funerals.",
    "{a} tells {b} a joke about a priest and a landlord. The landlord wins.",
    "{a} asks {b} how they're doing. {b} says 'you know'. {a} does.",
    "{a} confesses to {b} that they still don't know what their job is for.",
    "{a} and {b} agree to get coffee sometime. They both know they won't.",
];

const INSULTS: [&str; 10] = [
    "{a} tells {b} that their shoes look like a cry for help.",
    "{a} calls {b} 'a rounding error of a person'.",
    "{a} asks {b} if they've considered being someone else.",
    "{a} loudly guesses {b}'s salary. It is accurate.",
    "{a} tells {b} their mother sounded disappointed on the phone.",
    "{a} says {b} smells like a bus station. {b} is visibly hurt.",
    "{a} calls {b} 'champ' in a tone that means the opposite.",
    "{a} tells {b} nobody would notice if they moved away.",
    "{a} mocks {b}'s laugh. Everyone heard.",
    "{a} implies {b} was the one who clogged the toilet. It was {a}.",
];

const PLAYER_CHATS: [&str; 10] = [
    "You and {b} agree that things used to be better. Neither of you can say when.",
    "You tell {b} about your week. {b} nods for slightly too long.",
    "{b} tells you about their landlord. You tell them about yours. It's the same man.",
    "You and {b} discuss the weather. It is grey.",
    "{b} asks if you've heard the hum. You say no. You both know you're lying.",
    "{b} shows you a photo of a dog. It is not their dog.",
    "You and {b} share a long silence. It is the best part of the conversation.",
    "{b} tells you that you look tired. You thank them, for some reason.",
    "{b} explains a get-rich-quick scheme to you. You ask good questions, which is worse.",
    "You and {b} agree to get coffee sometime. You both know you won't.",
];

const PLAYER_INSULTS: [&str; 6] = [
    "You tell {b} that their shoes look like a cry for help.",
    "You call {b} 'a rounding error of a person'.",
    "You ask {b} if they've considered being someone else.",
    "You loudly guess {b}'s salary. It is accurate.",
    "You tell {b} nobody would notice if they moved away.",
    "You call {b} 'champ' in a tone that means the opposite.",
];

const INSULTS_TO_PLAYER: [&str; 6] = [
    "{a} tells you that your shoes look like a cry for help.",
    "{a} calls you 'a rounding error of a person'.",
    "{a} asks if you've considered being someone else.",
    "{a} loudly guesses your salary. It is accurate.",
    "{a} tells you nobody would notice if you moved away.",
    "{a} says you smell like a bus station.",
];

const GOSSIP: [&str; 5] = [
    "{a} tells {b} what {x} did. It gets worse in the retelling.",
    "{a} leans in and tells {b} something about {x}. {b} gasps at the right moment.",
    "{a} warns {b} to stay away from {x}.",
    "{a} spreads a story about {x}. {b} will be spreading it by tonight.",
    "{a} tells {b} about {x}. {b} says they always knew.",
];

fn fill(t: &str, w: &World, a: usize, b: usize, x: Option<usize>) -> String {
    let an = w.cname(a);
    let bn = w.name(b);
    let mut s = t.replacen("{a}", &an, 1).replace("{a}", &w.name(a)).replace("{b}", &bn);
    if let Some(x) = x {
        s = s.replace("{x}", &w.name(x));
    }
    s
}

pub fn compatibility(a: &Person, b: &Person) -> i32 {
    let mut c = 0;
    if a.has(Trait::Kind) || b.has(Trait::Kind) {
        c += 2;
    }
    if a.has(Trait::Charming) {
        c += 2;
    }
    if a.has(Trait::Petty) {
        c -= 1;
    }
    if a.has(Trait::Hothead) && b.has(Trait::Hothead) {
        c -= 3;
    }
    if a.has(Trait::Gossip) && b.has(Trait::Gossip) {
        c += 3;
    }
    if a.has(Trait::Weird) != b.has(Trait::Weird) {
        c -= 1;
    }
    if a.has(Trait::Paranoid) {
        c -= 1;
    }
    c
}

/// Idle people sometimes strike up a conversation with someone adjacent.
pub fn maybe_chat(w: &mut World, i: usize, chance: f32) {
    let now = w.turn;
    if now - w.people[i].last_chat < 20 || !w.chance(chance) {
        return;
    }
    let pos = w.people[i].pos;
    let near: Vec<usize> = w
        .people
        .iter()
        .filter(|q| q.id != i && q.awake(now) && q.jailed_until <= now && q.pos.dist(pos) <= 1 && q.hostile_to.is_none())
        .filter(|q| !q.is_player || w.autopilot || q.awake(now) && w.busy.is_none())
        .map(|q| q.id)
        .collect();
    if let Some(&j) = near.choose(&mut w.rng) {
        let p = &w.people[i];
        let grumpy = (p.stress > 60.0 || p.drunk > 40.0) && (p.has(Trait::Petty) || p.has(Trait::Hothead)) || p.drunk > 55.0;
        let dislike = p.opinion(j) < -30;
        if (grumpy && w.chance(0.3)) || (dislike && w.chance(0.25)) {
            insult(w, i, j);
        } else {
            chat(w, i, j);
        }
    }
}

pub fn chat(w: &mut World, a: usize, b: usize) {
    let now = w.turn;
    let pos = w.people[a].pos;
    let c = compatibility(&w.people[a], &w.people[b]) + w.rng.gen_range(-4..=6);
    let ob = w.people[a].opinion(b) / 25;
    {
        let p = &mut w.people[a];
        p.adjust_opinion(b, c + ob);
        p.lonely = (p.lonely - 30.0).max(0.0);
        p.stress = (p.stress - 3.0).max(0.0);
        p.last_chat = now;
    }
    {
        let q = &mut w.people[b];
        q.adjust_opinion(a, c + ob);
        q.lonely = (q.lonely - 30.0).max(0.0);
        q.stress = (q.stress - 3.0).max(0.0);
        q.last_chat = now;
    }

    // Gossip: share the worst thing they've seen.
    let gossipy = w.people[a].has(Trait::Gossip);
    let mem = w.people[a]
        .memories
        .iter()
        .rev()
        .find(|m| {
            matches!(m.kind, MemKind::SawAssault | MemKind::SawMurder | MemKind::SawTheft | MemKind::Robbed | MemKind::Attacked | MemKind::SawFire | MemKind::Insulted)
                && m.about.is_some()
                && m.about != Some(b)
        })
        .cloned();
    if let Some(m) = mem {
        if w.chance(if gossipy { 0.7 } else { 0.25 }) {
            let x = m.about.unwrap();
            let sting = match m.kind {
                MemKind::SawMurder => 40,
                MemKind::SawAssault | MemKind::Attacked | MemKind::SawFire => 18,
                _ => 9,
            };
            let q = &mut w.people[b];
            q.adjust_opinion(x, -sting);
            q.remember(now, MemKind::HeardRumor, Some(x), m.victim);
            let t = *GOSSIP.choose(&mut w.rng).unwrap();
            let s = if a == w.player { fill("You tell {b} what {x} did. It gets worse in the retelling.", w, a, b, Some(x)) } else { fill(t, w, a, b, Some(x)) };
            let involves = [a, b];
            if x == w.player || w.chance(0.5) {
                w.say(Some(pos), &involves, LogKind::Speech, s);
            }
            return;
        }
    }
    if w.chance(0.35) || a == w.player || b == w.player {
        // Templates with the player are written from "your" side.
        let (ta, tb) = if b == w.player { (b, a) } else { (a, b) };
        let t = if ta == w.player { *PLAYER_CHATS.choose(&mut w.rng).unwrap() } else { *CHATS.choose(&mut w.rng).unwrap() };
        let s = fill(t, w, ta, tb, None);
        w.say(Some(pos), &[a, b], LogKind::Speech, s);
    }
}

pub fn insult(w: &mut World, a: usize, b: usize) {
    let now = w.turn;
    let pos = w.people[a].pos;
    let sting = w.rng.gen_range(12..30);
    let t = if a == w.player {
        *PLAYER_INSULTS.choose(&mut w.rng).unwrap()
    } else if b == w.player {
        *INSULTS_TO_PLAYER.choose(&mut w.rng).unwrap()
    } else {
        *INSULTS.choose(&mut w.rng).unwrap()
    };
    let s = fill(t, w, a, b, None);
    w.say(Some(pos), &[a, b], LogKind::Speech, s);
    {
        let p = &mut w.people[a];
        p.last_chat = now;
        p.adjust_opinion(b, -5);
    }
    let fight = {
        let q = &mut w.people[b];
        q.adjust_opinion(a, -sting);
        q.stress = (q.stress + 10.0).min(100.0);
        q.remember(now, MemKind::Insulted, Some(a), Some(b));
        q.last_chat = now;
        let o = q.opinion(a);
        !q.is_player
            && !q.has(Trait::Coward)
            && (q.has(Trait::Hothead) && o < -15 || o < -40 && (q.drunk > 25.0 || q.stress > 55.0) || o < -75)
    };
    if fight && w.chance(0.6) {
        let bn = w.people[b].name.clone();
        let an = w.name(a);
        w.say(Some(pos), &[a, b], LogKind::Danger, format!("{} has had enough of {}.", bn, an));
        let q = &mut w.people[b];
        q.hostile_to = Some(a);
        q.hostile_until = now + 40;
    }
}

pub fn compliment(w: &mut World, a: usize, b: usize) {
    let pos = w.people[a].pos;
    let charming = w.people[a].has(Trait::Charming);
    let paranoid = w.people[b].has(Trait::Paranoid);
    let ok = w.chance(if charming { 0.85 } else { 0.6 } - if paranoid { 0.3 } else { 0.0 });
    let (an, bn) = (w.cname(a), w.name(b));
    if ok {
        let d = w.rng.gen_range(6..14);
        w.people[b].adjust_opinion(a, d);
        w.people[b].stress = (w.people[b].stress - 5.0).max(0.0);
        w.say(Some(pos), &[a, b], LogKind::Speech, format!("{} compliment {}'s coat. They stand a little straighter.", an, bn));
    } else {
        w.people[b].adjust_opinion(a, -4);
        w.say(Some(pos), &[a, b], LogKind::Speech, format!("{} compliment {}. It lands wrong. They check their wallet.", an, bn));
    }
}

pub fn smoke(w: &mut World, i: usize) {
    let pos = w.people[i].pos;
    let p = &mut w.people[i];
    if !p.inventory.contains(&ItemKind::Cigarettes) {
        return;
    }
    p.stress = (p.stress - 8.0).max(0.0);
    let risk = 0.03 * (1.0 + p.drunk / 30.0);
    if w.rng.gen::<f32>() < risk {
        // Flicked without looking.
        let spot = DIRS8.iter().map(|(dx, dy)| pos.add(*dx, *dy)).find(|q| w.map.in_bounds(*q) && w.map.get(*q).fuel > 0);
        if let Some(s) = spot {
            if fire::ignite(w, s) {
                let n = w.cname(i);
                w.say(Some(pos), &[i], LogKind::Danger, format!("{} flick{} a cigarette away without looking.", n, if i == w.player { "" } else { "s" }));
            }
        }
    }
}
