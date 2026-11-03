//! Everything the player can do. Each successful action advances the world one turn.

use crate::combat;
use crate::events;
use crate::fire;
use crate::map::{BuildingKind, Furniture, Pos};
use crate::person::*;
use crate::social;
use crate::world::*;
use rand::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interact {
    Chat,
    Compliment,
    Insult,
    AskWork,
    AskMoney,
    Pickpocket,
    Attack,
}

impl Interact {
    pub const ALL: [Interact; 7] =
        [Interact::Chat, Interact::Compliment, Interact::Insult, Interact::AskWork, Interact::AskMoney, Interact::Pickpocket, Interact::Attack];
    pub fn label(self) -> &'static str {
        match self {
            Interact::Chat => "Chat",
            Interact::Compliment => "Compliment",
            Interact::Insult => "Insult",
            Interact::AskWork => "Ask about work",
            Interact::AskMoney => "Ask to borrow money",
            Interact::Pickpocket => "Pickpocket",
            Interact::Attack => "Attack",
        }
    }
}

/// What lies in a direction from the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bump {
    Move,
    Person(usize),
    Hostile(usize),
    Furniture(Furniture, Pos),
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShopKind {
    Store,
    Diner,
    Bar,
    None,
}

impl World {
    pub fn player_pos(&self) -> Pos {
        self.people[self.player].pos
    }

    pub fn bump(&self, dx: i32, dy: i32) -> Bump {
        let pid = self.player;
        let to = self.player_pos().add(dx, dy);
        if !self.map.in_bounds(to) {
            return Bump::Blocked;
        }
        if let Some(n) = self.living_at(to) {
            let hostile = self.people[n].hostile_to == Some(pid) || self.people[pid].hostile_to == Some(n);
            return if hostile { Bump::Hostile(n) } else { Bump::Person(n) };
        }
        let t = self.map.get(to);
        if !t.furn.walkable() && t.furn != Furniture::None {
            return Bump::Furniture(t.furn, to);
        }
        if self.map.walkable(to) {
            Bump::Move
        } else {
            Bump::Blocked
        }
    }

    pub fn shop_here(&self, at: Pos) -> ShopKind {
        match self.map.building_at(at).map(|b| self.map.buildings[b].kind) {
            Some(BuildingKind::Store) => ShopKind::Store,
            Some(BuildingKind::Diner) => ShopKind::Diner,
            Some(BuildingKind::Bar) => ShopKind::Bar,
            _ => ShopKind::None,
        }
    }

    fn act_done(&mut self) {
        self.step();
    }

    pub fn act_move(&mut self, dx: i32, dy: i32) {
        let pid = self.player;
        let p = &self.people[pid];
        let to = p.pos.add(dx, dy);
        if !p.body.legs_ok() && self.turn % 2 == 0 {
            self.say(None, &[pid], LogKind::Bad, "You drag yourself along the ground.");
            self.act_done();
            return;
        }
        if self.map.walkable(to) {
            // Diagonal squeeze past corners isn't allowed.
            if dx != 0 && dy != 0 && (!self.map.walkable(p.pos.add(dx, 0)) && !self.map.walkable(p.pos.add(0, dy))) {
                return;
            }
            self.people[pid].pos = to;
            if self.map.get(to).fire > 0 {
                self.say(None, &[pid], LogKind::Danger, "You step into the flames. Bold.");
            }
            self.act_done();
        }
    }

    pub fn act_wait(&mut self, minutes: u64) {
        if minutes <= 1 {
            self.act_done();
        } else {
            self.busy = Some(Busy::Wait(self.turn + minutes));
        }
    }

    pub fn act_sleep(&mut self) {
        let pid = self.player;
        let p = &self.people[pid];
        if p.energy > 85.0 {
            self.say(None, &[pid], LogKind::Normal, "You lie there with your eyes open. You're not tired enough to sleep, only to want to.");
            return;
        }
        let on_bed = matches!(self.map.get(p.pos).furn, Furniture::Bed | Furniture::Bench | Furniture::Sofa);
        if Some(p.pos) == p.bed {
            self.say(None, &[pid], LogKind::Normal, "You go to bed.");
        } else if on_bed {
            self.say(None, &[pid], LogKind::Normal, "You lie down somewhere that isn't yours.");
        } else {
            self.say(None, &[pid], LogKind::Normal, "You lie down on the ground. It is not the first time.");
            self.people[pid].stress = (self.people[pid].stress + 5.0).min(100.0);
        }
        self.people[pid].sleeping = true;
        self.busy = Some(Busy::Sleep);
    }

    pub fn act_work(&mut self) {
        let pid = self.player;
        let p = &self.people[pid];
        let Some(j) = p.job else {
            self.say(None, &[pid], LogKind::Normal, "You don't have a job. Ask around: people know when someone's position opens up.");
            return;
        };
        let job = self.jobs[j].clone();
        let wp = p.workplace.map(|b| self.map.buildings[b].name.clone()).unwrap_or_default();
        if !self.on_shift(pid) {
            let when = self.minutes_until_shift(pid).map(|m| format!("in {}h {:02}m", m / 60, m % 60)).unwrap_or("never".into());
            self.say(None, &[pid], LogKind::Normal, format!("You're not on shift. Your next shift at {} starts {}.", wp, when));
            return;
        }
        let spot = p.work_spot.unwrap_or(p.pos);
        if p.pos.dist(spot) > 1 {
            self.say(None, &[pid], LogKind::Normal, format!("You need to be at your station at {} (marked on the map).", wp));
            return;
        }
        if p.drunk > 60.0 {
            self.say(None, &[pid], LogKind::Bad, "You try to work, but the room keeps leaning.");
        }
        self.say(None, &[pid], LogKind::Normal, format!("You clock in as {}. The hours begin to pass.", job.name));
        self.busy = Some(Busy::Work);
    }

    pub fn act_interact(&mut self, n: usize, what: Interact) {
        let pid = self.player;
        let now = self.turn;
        if !self.people[n].alive {
            return;
        }
        let pos = self.people[n].pos;
        let nn = self.people[n].name.clone();
        let asleep = !self.people[n].awake(now);
        if asleep && !matches!(what, Interact::Pickpocket | Interact::Attack) {
            self.say(Some(pos), &[pid], LogKind::Normal, format!("{} is not in a state to talk.", nn));
            return;
        }
        match what {
            Interact::Chat => {
                social::chat(self, pid, n);
                events::maybe_offer(self, n, true);
            }
            Interact::Compliment => social::compliment(self, pid, n),
            Interact::Insult => social::insult(self, pid, n),
            Interact::AskWork => self.ask_work(n),
            Interact::AskMoney => {
                let o = self.people[n].opinion(pid);
                if o >= 30 && self.people[n].money > 60 {
                    let amt = self.rng.gen_range(10..35);
                    self.people[n].money -= amt;
                    self.people[pid].money += amt;
                    self.people[n].adjust_opinion(pid, -8);
                    self.say(Some(pos), &[pid, n], LogKind::Good, format!("{} hands you ${} without meeting your eyes.", nn, amt));
                } else {
                    self.people[n].adjust_opinion(pid, -5);
                    self.say(Some(pos), &[pid, n], LogKind::Speech, format!("{} laughs, then realises you're serious, then stops laughing.", nn));
                }
            }
            Interact::Pickpocket => {
                let q = &self.people[n];
                let chance = if asleep { 0.9 } else { 0.45 } + if q.drunk > 40.0 { 0.2 } else { 0.0 } - if q.has(Trait::Paranoid) { 0.25 } else { 0.0 };
                if self.chance(chance) {
                    let have = self.people[n].money.max(0);
                    let amt = if have == 0 { 0 } else { self.rng.gen_range(1..=have.min(60)) };
                    self.people[n].money -= amt;
                    self.people[pid].money += amt;
                    if amt == 0 {
                        self.say(Some(pos), &[pid], LogKind::Normal, format!("You go through {}'s pockets. Lint, a bus ticket, nothing.", nn));
                    } else {
                        self.say(Some(pos), &[pid], LogKind::Good, format!("You lift ${} from {}. They don't notice.", amt, nn));
                    }
                } else {
                    self.people[n].adjust_opinion(pid, -40);
                    self.say(Some(pos), &[pid, n], LogKind::Danger, format!("{} catches your hand in their pocket.", nn));
                    if self.people[n].has(Trait::Hothead) || self.people[n].has(Trait::Brave) {
                        self.people[n].hostile_to = Some(pid);
                        self.people[n].hostile_until = now + 40;
                    }
                    self.crime(CrimeKind::Pickpocket, pid, Some(n), pos);
                }
            }
            Interact::Attack => combat::attack(self, pid, n),
        }
        self.act_done();
    }

    fn ask_work(&mut self, n: usize) {
        let pid = self.player;
        let pos = self.people[n].pos;
        let nn = self.people[n].name.clone();
        let Some(b) = self.people[n].workplace else {
            self.say(Some(pos), &[pid, n], LogKind::Speech, format!("{} doesn't have a job either. You stand there in solidarity.", nn));
            return;
        };
        let kind = self.map.buildings[b].kind;
        let bn = self.map.buildings[b].name.clone();
        let my_tier = self.tier(pid);
        let record = self.people[pid].record;
        let mut options: Vec<usize> = (0..self.jobs.len())
            .filter(|&j| self.jobs[j].building == kind && self.job_vacant(j) && Some(j) != self.people[pid].job)
            .filter(|&j| self.jobs[j].tier <= my_tier + 1)
            .collect();
        options.sort_by_key(|&j| std::cmp::Reverse(self.jobs[j].tier));
        if self.people[n].opinion(pid) < -15 {
            self.say(Some(pos), &[pid, n], LogKind::Speech, format!("{} says there's nothing. You can tell there is.", nn));
            return;
        }
        let Some(&j) = options.first() else {
            self.say(Some(pos), &[pid, n], LogKind::Speech, format!("{}: \"{} isn't hiring. Someone would have to die first.\"", nn, bn));
            return;
        };
        if record > 0 && self.jobs[j].tier >= 2 {
            self.say(Some(pos), &[pid, n], LogKind::Speech, format!("{}: \"There's an opening, but not for someone with a record.\"", nn));
            return;
        }
        let old = self.job_name(pid).to_string();
        self.assign_job(pid, j);
        let job = self.jobs[j].clone();
        let quit = if old != "Unemployed" { format!(" You quit being a {} by not going back.", old) } else { String::new() };
        self.say(
            Some(pos),
            &[pid, n],
            LogKind::Good,
            format!(
                "{} puts in a word. You're hired at {} as {}: ${}/shift, starting {:02}:00.{}",
                nn, bn, job.name, job.wage, job.start, quit
            ),
        );
    }

    /// Buy something at the counter/register you're standing at.
    pub fn act_buy(&mut self, item: Option<ItemKind>) {
        let pid = self.player;
        let pos = self.player_pos();
        let b = self.map.building_at(pos);
        let shop = self.shop_here(pos);
        let (price, what) = match (shop, item) {
            (ShopKind::Diner, _) => (9, "meal"),
            (ShopKind::Bar, _) => (6, "drink"),
            (ShopKind::Store, Some(it)) => (it.price(), it.name()),
            _ => return,
        };
        if self.people[pid].money < price {
            self.say(None, &[pid], LogKind::Bad, format!("You can't afford the {} (${}). You pretend to read the menu.", what, price));
            return;
        }
        self.people[pid].money -= price;
        if let Some(b) = b {
            self.map.buildings[b].till += price;
        }
        let p = &mut self.people[pid];
        match shop {
            ShopKind::Diner => {
                p.hunger = (p.hunger - 65.0).max(0.0);
                p.stress = (p.stress - 4.0).max(0.0);
                self.say(None, &[pid], LogKind::Normal, "You eat the special. It is beige. You feel briefly human.");
                self.busy = Some(Busy::Wait(self.turn + 20));
            }
            ShopKind::Bar => {
                p.drunk += 20.0;
                p.stress = (p.stress - 14.0).max(0.0);
                let lines = ["You drink. The edges of things get softer.", "You drink something brown. It tastes like a decision.", "You drink. The bartender doesn't ask, which is the service."];
                let l = lines[self.rng.gen_range(0..3)];
                self.say(None, &[pid], LogKind::Normal, l);
                self.busy = Some(Busy::Wait(self.turn + 15));
            }
            ShopKind::Store => {
                p.inventory.push(item.unwrap());
                self.say(None, &[pid], LogKind::Normal, format!("You buy a {} for ${}.", what, price));
                self.act_done();
            }
            ShopKind::None => {}
        }
    }

    pub fn act_steal_till(&mut self, reg: Pos) {
        let pid = self.player;
        let Some(b) = self.map.building_at(reg) else { return };
        let take = self.map.buildings[b].till.max(0);
        self.map.buildings[b].till = 0;
        self.people[pid].money += take;
        self.stats.player_robberies += 1;
        if !self.robbed.contains(&b) {
            self.robbed.push(b);
        }
        let bn = self.map.buildings[b].name.clone();
        self.say(None, &[pid], LogKind::Danger, format!("You empty the till at {}: ${}. Your heart is very loud.", bn, take));
        self.crime(CrimeKind::Theft, pid, None, reg);
        self.act_done();
    }

    pub fn act_ignite(&mut self, dx: i32, dy: i32) {
        let pid = self.player;
        if !self.people[pid].inventory.contains(&ItemKind::Lighter) {
            self.say(None, &[pid], LogKind::Normal, "You'd need a lighter. The corner store sells them for $2, no questions.");
            return;
        }
        let at = self.player_pos().add(dx, dy);
        if fire::ignite(self, at) {
            self.stats.player_fires += 1;
            let what = self.map.get(at).furn.name();
            let what = if what.is_empty() { self.map.get(at).terrain.name() } else { what };
            self.say(None, &[pid], LogKind::Danger, format!("You hold the lighter to the {}. It catches.", what));
            self.crime(CrimeKind::Arson, pid, None, at);
        } else {
            self.say(None, &[pid], LogKind::Normal, "That won't burn. You feel a strange disappointment.");
        }
        self.act_done();
    }

    pub fn act_use(&mut self, idx: usize) {
        let pid = self.player;
        let Some(&item) = self.people[pid].inventory.get(idx) else { return };
        let mut consume = true;
        let msg: String = match item {
            ItemKind::Snack => {
                let p = &mut self.people[pid];
                p.hunger = (p.hunger - 35.0).max(0.0);
                "You eat the sandwich. It was a sandwich in the legal sense.".into()
            }
            ItemKind::Beer => {
                let p = &mut self.people[pid];
                p.drunk += 18.0;
                p.stress = (p.stress - 8.0).max(0.0);
                "You drink the beer. Warm. Of course.".into()
            }
            ItemKind::Cigarettes => {
                consume = false;
                social::smoke(self, pid);
                if !self.people[pid].inventory.contains(&ItemKind::Lighter) {
                    "You have nothing to light it with. You hold it in your mouth anyway, for the look.".into()
                } else {
                    "You smoke. For four minutes, you are someone in a film.".into()
                }
            }
            ItemKind::Lighter => {
                consume = false;
                "Flick. The flame looks back at you. (Use F + direction to set something alight.)".into()
            }
            ItemKind::Knife => {
                consume = false;
                "You hold the knife. You are now a person holding a knife. (Carried knives are used when you attack.)".into()
            }
            ItemKind::Bandage => {
                let p = &mut self.people[pid];
                p.body.bleeding = 0;
                p.body.hp[TORSO] = (p.body.hp[TORSO] + 2).min(p.body.max[TORSO]);
                "You wrap the wound. It stops bleeding, mostly out of politeness.".into()
            }
            ItemKind::Painkillers => {
                let p = &mut self.people[pid];
                p.stress = (p.stress - 15.0).max(0.0);
                for k in 0..6 {
                    p.body.hp[k] = (p.body.hp[k] + 1).min(p.body.max[k]);
                }
                "You dry-swallow two pills. The world goes slightly cotton.".into()
            }
            ItemKind::Lottery => {
                if self.chance(0.04) {
                    self.people[pid].money += 250;
                    "You WIN $250. The universe has made a clerical error.".into()
                } else {
                    "You scratch off a picture of a frowning sun. You lose. The sun is also disappointed.".into()
                }
            }
        };
        if consume {
            self.people[pid].inventory.remove(idx);
        }
        self.say(None, &[pid], LogKind::Normal, msg);
        self.act_done();
    }

    pub fn act_answer_phone(&mut self) {
        events::answer_phone(self);
        self.act_done();
    }

    pub fn accept_offer(&mut self) {
        events::accept_offer(self);
    }
    pub fn decline_offer(&mut self) {
        events::decline_offer(self);
    }

    pub fn player_alive(&self) -> bool {
        self.people[self.player].alive
    }

    /// Short description of what someone is doing, for the look panel.
    pub fn describe_activity(&self, i: usize) -> String {
        let p = &self.people[i];
        let now = self.turn;
        if !p.alive {
            return format!("dead ({})", p.cause_of_death.clone().unwrap_or_default());
        }
        if p.jailed_until > now {
            return "in a cell".into();
        }
        if p.unconscious_until > now {
            return "unconscious".into();
        }
        if p.sleeping {
            return "asleep".into();
        }
        match p.intent.kind {
            IntentKind::Work => if p.intent.started { "working" } else { "heading to work" }.into(),
            IntentKind::Sleep => "heading to bed".into(),
            IntentKind::Eat | IntentKind::EatHome => "eating".into(),
            IntentKind::Drink => "drinking".into(),
            IntentKind::Socialize => "looking for company".into(),
            IntentKind::Clinic => "seeking medical help".into(),
            IntentKind::Shop => "shopping".into(),
            IntentKind::Steal => "acting suspiciously".into(),
            IntentKind::Scavenge => "going through the bins".into(),
            IntentKind::Flee => "fleeing".into(),
            IntentKind::Fight(t) => format!("fighting {}", self.name(t)),
            IntentKind::Chase(t) => format!("pursuing {}", self.name(t)),
            IntentKind::Extinguish => "fighting the fire".into(),
            IntentKind::Arson => "walking very calmly".into(),
            IntentKind::Wander => "wandering".into(),
            IntentKind::Loiter => "killing time".into(),
            IntentKind::None => "standing around".into(),
        }
    }

    pub fn describe_condition(&self, i: usize) -> Vec<&'static str> {
        let p = &self.people[i];
        let mut v = vec![];
        if !p.alive {
            return v;
        }
        let h = p.body.health_frac();
        if h < 0.4 {
            v.push("badly hurt");
        } else if h < 0.8 {
            v.push("bruised");
        }
        if p.body.bleeding > 0 {
            v.push("bleeding");
        }
        if p.drunk > 60.0 {
            v.push("very drunk");
        } else if p.drunk > 25.0 {
            v.push("tipsy");
        }
        if p.energy < 20.0 {
            v.push("exhausted");
        }
        if p.hunger > 75.0 {
            v.push("starving");
        }
        if p.stress > 75.0 {
            v.push("on the edge");
        } else if p.stress > 50.0 {
            v.push("tense");
        }
        if p.hum {
            v.push("listening to something");
        }
        v
    }
}
