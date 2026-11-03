//! End-to-end checks of the player-facing actions and the city's reactions.

use greycity_sim::map::{BuildingKind, Furniture, Pos};
use greycity_sim::person::ItemKind;
use greycity_sim::player::{Bump, Interact};
use greycity_sim::world::{Busy, LogKind, Role, StartJob, World};

fn world() -> World {
    World::new(42, StartJob::Random)
}

fn place_next_to(w: &mut World, n: usize) -> (i32, i32) {
    let np = w.people[n].pos;
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let p = np.add(-dx, -dy);
        if w.map.walkable(p) && w.living_at(p).is_none() {
            w.people[w.player].pos = p;
            return (dx, dy);
        }
    }
    panic!("no free tile next to npc");
}

fn awake_npc(w: &World) -> usize {
    (0..w.people.len()).find(|&i| i != w.player && w.people[i].awake(w.turn) && w.role(i) != Role::Cop).unwrap()
}

#[test]
fn deterministic_for_same_seed() {
    let mut a = World::new(9, StartJob::Random);
    let mut b = World::new(9, StartJob::Random);
    a.autopilot = true;
    b.autopilot = true;
    for _ in 0..2000 {
        a.step();
        b.step();
    }
    let la: Vec<_> = a.log.iter().map(|e| e.text.clone()).collect();
    let lb: Vec<_> = b.log.iter().map(|e| e.text.clone()).collect();
    assert_eq!(la, lb);
}

#[test]
fn bump_and_chat() {
    let mut w = world();
    let n = awake_npc(&w);
    let (dx, dy) = place_next_to(&mut w, n);
    assert!(matches!(w.bump(dx, dy), Bump::Person(x) if x == n));
    let before = w.turn;
    w.act_interact(n, Interact::Chat);
    assert_eq!(w.turn, before + 1);
    assert!(w.people[n].opinions.contains_key(&w.player));
}

#[test]
fn attacking_is_a_crime_and_cops_come() {
    let mut w = world();
    let n = awake_npc(&w);
    place_next_to(&mut w, n);
    for _ in 0..3 {
        w.act_interact(n, Interact::Attack);
    }
    assert!(w.crimes.iter().any(|c| c.perp == w.player));
    // Let the city respond for a while; the player stands still.
    w.people[n].hostile_to = None;
    for _ in 0..600 {
        w.step();
        if w.people[w.player].jailed_until > w.turn {
            break;
        }
    }
    let p = &w.people[w.player];
    let reported = w.crimes.iter().any(|c| c.perp == w.player && c.reported);
    assert!(!reported || p.jailed_until > w.turn || !p.alive || w.crimes.iter().all(|c| c.resolved), "reported crime but no arrest");
}

#[test]
fn shopping_and_items() {
    let mut w = world();
    let store = w.map.first_of(BuildingKind::Store).unwrap();
    let spot = w.map.buildings[store].service[0];
    w.people[w.player].pos = spot;
    w.people[w.player].money = 100;
    w.act_buy(Some(ItemKind::Lighter));
    assert!(w.people[w.player].inventory.contains(&ItemKind::Lighter));
    assert_eq!(w.people[w.player].money, 100 - ItemKind::Lighter.price());
    // Lottery ticket the player starts with can be used.
    let k = w.people[w.player].inventory.iter().position(|i| *i == ItemKind::Lottery).unwrap();
    w.act_use(k);
    assert!(!w.people[w.player].inventory.contains(&ItemKind::Lottery));
}

#[test]
fn robbing_the_till() {
    let mut w = world();
    let store = w.map.first_of(BuildingKind::Store).unwrap();
    let reg = w.map.buildings[store].register.unwrap();
    assert_eq!(w.map.get(reg).furn, Furniture::Register);
    let before = w.people[w.player].money;
    w.people[w.player].pos = w.map.buildings[store].service[0];
    w.act_steal_till(reg);
    assert!(w.people[w.player].money > before);
    assert_eq!(w.stats.player_robberies, 1);
}

#[test]
fn arson_spreads_and_gets_called_in() {
    let mut w = world();
    let bar = w.map.first_of(BuildingKind::Bar).unwrap();
    let seat = w.map.buildings[bar].seats[3];
    w.people[w.player].pos = seat;
    w.people[w.player].inventory.push(ItemKind::Lighter);
    // Find a flammable neighbour.
    let (dx, dy) = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1)]
        .into_iter()
        .find(|(dx, dy)| w.map.get(seat.add(*dx, *dy)).fuel > 0)
        .unwrap();
    w.act_ignite(dx, dy);
    assert_eq!(w.stats.player_fires, 1);
    w.people[w.player].pos = Pos::new(1, 1); // walk away, whistling
    for _ in 0..120 {
        w.step();
    }
    assert!(w.stats.fires >= 1);
    assert!(w.log.iter().any(|e| e.text.contains("fire department")), "nobody called the fire in");
}

#[test]
fn sleeping_fast_forwards_until_rested() {
    let mut w = world();
    let pid = w.player;
    w.people[pid].job = None; // no alarm clock
    w.people[pid].energy = 30.0;
    w.act_sleep();
    assert_eq!(w.busy, Some(Busy::Sleep));
    let start = w.turn;
    while w.busy.is_some() && w.turn < start + 1440 {
        w.step();
    }
    assert!(w.turn - start > 200);
    assert!(w.people[w.player].energy > 90.0 || w.busy.is_none());
}

#[test]
fn offers_can_be_accepted() {
    let mut w = world();
    // Make an NPC who hates someone else and likes the player.
    let n = awake_npc(&w);
    let victim = (0..w.people.len()).find(|&i| i != n && i != w.player && w.people[i].alive).unwrap();
    w.people[n].adjust_opinion(victim, -100);
    w.people[n].adjust_opinion(w.player, 60);
    w.people[w.player].money = 0;
    for _ in 0..50 {
        greycity_sim::events::maybe_offer(&mut w, n, true);
        if w.offer.is_some() {
            break;
        }
    }
    assert!(w.offer.is_some(), "no offer was made");
    w.accept_offer();
    assert_eq!(w.objectives.len(), 1);
    // Fulfil it the hard way.
    let pid = w.player;
    w.kill(victim, "test".into(), Some(pid));
    w.step();
    assert!(w.objectives[0].done);
    assert!(w.log.iter().any(|e| e.kind == LogKind::Offer));
}

#[test]
fn a_month_of_city_life_does_not_collapse() {
    let mut w = World::new(3, StartJob::Random);
    w.autopilot = true;
    for _ in 0..30 * 1440 {
        w.step();
    }
    let alive = w.people.iter().filter(|p| p.alive).count();
    assert!(alive > w.target_population / 2, "city depopulated: {} alive", alive);
}
