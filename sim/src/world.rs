//! The World: everything that exists, and the clock that grinds it forward.

use crate::map::{self, BuildingKind, Map, Pos, SpotKind, Terrain};
use crate::path;
use crate::person::*;
use crate::{ai, events, fire};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;
use serde::Deserialize;

pub const MINUTES_PER_DAY: u64 = 1440;
pub const START_TURN: u64 = 6 * 60 + 30;
pub const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Role {
    #[default]
    Worker,
    Cop,
    Firefighter,
    Medic,
    Cook,
    Janitor,
    Cashier,
    Bartender,
    Security,
}

#[derive(Deserialize, Clone, Debug)]
pub struct JobDef {
    pub name: String,
    pub tier: u8,
    pub building: BuildingKind,
    pub spot: SpotKind,
    pub start: u32,
    pub hours: u32,
    pub wage: i32,
    pub slots: u32,
    #[serde(default)]
    pub promotes_to: Option<String>,
    #[serde(default)]
    pub role: Role,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub enum WeirdEffect {
    Message,
    StressAll(i32),
    StressPlayer(i32),
    HumTouch,
    ElectricalFire,
    Windfall(i32),
    Doppelganger,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WeirdDef {
    pub weight: u32,
    pub effect: WeirdEffect,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogKind {
    Normal,
    Info,
    Good,
    Bad,
    Danger,
    Weird,
    Speech,
    Offer,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub turn: u64,
    pub text: String,
    pub kind: LogKind,
    pub seen: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CrimeKind {
    Assault,
    Murder,
    Theft,
    Pickpocket,
    Arson,
}

impl CrimeKind {
    pub fn severity(self) -> i32 {
        match self {
            CrimeKind::Pickpocket => 1,
            CrimeKind::Theft => 2,
            CrimeKind::Assault => 3,
            CrimeKind::Arson => 5,
            CrimeKind::Murder => 8,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            CrimeKind::Assault => "assault",
            CrimeKind::Murder => "murder",
            CrimeKind::Theft => "theft",
            CrimeKind::Pickpocket => "pickpocketing",
            CrimeKind::Arson => "arson",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Crime {
    pub kind: CrimeKind,
    pub perp: usize,
    pub victim: Option<usize>,
    pub pos: Pos,
    pub turn: u64,
    pub reported: bool,
    pub resolved: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OfferSource {
    Person(usize),
    Payphone,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObjKind {
    Hurt(usize),
    Kill(usize),
    Burn(usize),
    Rob(usize),
    Fires(u32),
}

#[derive(Clone, Debug)]
pub struct Objective {
    pub source: OfferSource,
    pub kind: ObjKind,
    pub reward: i32,
    pub deadline: u64,
    pub baseline: u32,
    pub summary: String,
    pub done: bool,
    pub failed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Busy {
    Sleep,
    Work,
    Wait(u64),
    Jail,
}

#[derive(Clone, Copy, Debug)]
pub enum StartJob {
    Random,
    Unemployed,
    Job(usize),
}

#[derive(Default, Clone, Debug)]
pub struct Stats {
    pub deaths: u32,
    pub fires: u32,
    pub fights: u32,
    pub arrests: u32,
    pub player_fires: u32,
    pub player_robberies: u32,
    pub player_shifts: u32,
    pub promotions: u32,
}

pub struct World {
    pub map: Map,
    pub people: Vec<Person>,
    pub jobs: Vec<JobDef>,
    pub weird: Vec<WeirdDef>,
    pub turn: u64,
    pub rng: Pcg64,
    pub seed: u64,
    pub log: Vec<LogEntry>,
    pub player: usize,
    pub visible: Vec<bool>,
    pub seen: Vec<bool>,
    pub crimes: Vec<Crime>,
    pub burning: Vec<Pos>,
    pub fire_alarm: bool,
    pub offer: Option<(Objective, String)>,
    pub objectives: Vec<Objective>,
    pub busy: Option<Busy>,
    pub interrupt: bool,
    pub stats: Stats,
    pub phone_ringing: Option<(Pos, u64)>,
    pub autopilot: bool,
    pub target_population: usize,
    pub robbed: Vec<usize>,
}

pub fn load_jobs() -> Vec<JobDef> {
    ron::from_str(include_str!("../data/jobs.ron")).expect("data/jobs.ron is malformed")
}
pub fn load_weird() -> Vec<WeirdDef> {
    ron::from_str(include_str!("../data/weird.ron")).expect("data/weird.ron is malformed")
}

impl World {
    pub fn new(seed: u64, start: StartJob) -> World {
        let mut rng = Pcg64::seed_from_u64(seed);
        let map = map::generate(&mut rng);
        let n = (map.w * map.h) as usize;
        let mut w = World {
            map,
            people: vec![],
            jobs: load_jobs(),
            weird: load_weird(),
            turn: START_TURN,
            rng,
            seed,
            log: vec![],
            player: 0,
            visible: vec![false; n],
            // You've lived here. You know the streets, if not what's on them right now.
            seen: vec![true; n],
            crimes: vec![],
            burning: vec![],
            fire_alarm: false,
            offer: None,
            objectives: vec![],
            busy: None,
            interrupt: false,
            stats: Stats::default(),
            phone_ringing: None,
            autopilot: false,
            target_population: 0,
            robbed: vec![],
        };
        w.populate(start);
        w.target_population = w.people.len();
        w.update_fov();
        w.intro();
        w
    }

    // ------------------------------------------------------------------ time

    pub fn minute(&self) -> u64 {
        self.turn % MINUTES_PER_DAY
    }
    pub fn hour(&self) -> u32 {
        (self.minute() / 60) as u32
    }
    pub fn day(&self) -> u64 {
        self.turn / MINUTES_PER_DAY
    }
    pub fn clock(&self) -> String {
        format!("{:02}:{:02}", self.hour(), self.minute() % 60)
    }
    pub fn weekday(&self) -> &'static str {
        WEEKDAYS[(self.day() % 7) as usize]
    }
    pub fn is_night(&self) -> bool {
        let h = self.hour();
        !(7..20).contains(&h)
    }

    fn workday(&self, i: usize, day: u64) -> bool {
        if self.people[i].is_player {
            day % 7 < 5
        } else {
            (day + i as u64 * 3) % 7 < 5
        }
    }

    pub fn on_shift_at(&self, i: usize, t: u64) -> bool {
        let Some(j) = self.people[i].job else { return false };
        let job = &self.jobs[j];
        let m = (t % MINUTES_PER_DAY) as u32;
        let day = t / MINUTES_PER_DAY;
        let start = job.start * 60;
        let end = start + job.hours * 60;
        let (inside, sday) = if end <= 1440 {
            (m >= start && m < end, day)
        } else if m >= start {
            (true, day)
        } else if m < end - 1440 {
            (true, day.saturating_sub(1))
        } else {
            (false, day)
        };
        inside && self.workday(i, sday)
    }
    pub fn on_shift(&self, i: usize) -> bool {
        self.on_shift_at(i, self.turn)
    }
    pub fn minutes_until_shift(&self, i: usize) -> Option<u64> {
        self.people[i].job?;
        (0..7 * MINUTES_PER_DAY).step_by(15).find(|dt| self.on_shift_at(i, self.turn + dt))
    }
    pub fn role(&self, i: usize) -> Role {
        self.people[i].job.map(|j| self.jobs[j].role).unwrap_or(Role::Worker)
    }
    pub fn job_name(&self, i: usize) -> &str {
        self.people[i].job.map(|j| self.jobs[j].name.as_str()).unwrap_or("Unemployed")
    }
    pub fn tier(&self, i: usize) -> u8 {
        self.people[i].job.map(|j| self.jobs[j].tier).unwrap_or(0)
    }
    pub fn on_duty(&self, i: usize, role: Role) -> bool {
        self.people[i].alive && self.role(i) == role && self.on_shift(i)
    }

    // ------------------------------------------------------------------ log

    pub fn say(&mut self, at: Option<Pos>, involves: &[usize], kind: LogKind, text: impl Into<String>) {
        let seen = involves.contains(&self.player) || at.map(|p| self.visible[self.map.idx(p)]).unwrap_or(true);
        if seen && matches!(kind, LogKind::Danger | LogKind::Offer) {
            self.interrupt = true;
        }
        self.log.push(LogEntry { turn: self.turn, text: text.into(), kind, seen });
        if self.log.len() > 4000 {
            self.log.drain(0..1000);
        }
    }
    pub fn name(&self, i: usize) -> String {
        if i == self.player {
            "you".to_string()
        } else {
            self.people[i].name.clone()
        }
    }
    /// Capitalised name for the start of a sentence.
    pub fn cname(&self, i: usize) -> String {
        if i == self.player {
            "You".to_string()
        } else {
            self.people[i].name.clone()
        }
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.rng.gen::<f32>() < p
    }

    // ------------------------------------------------------------------ setup

    fn random_traits(&mut self) -> Vec<Trait> {
        let mut out: Vec<Trait> = vec![];
        let k = self.rng.gen_range(2..=3);
        while out.len() < k {
            let t = *Trait::ALL.choose(&mut self.rng).unwrap();
            if !out.contains(&t) && !out.iter().any(|o| o.conflicts(t)) {
                out.push(t);
            }
        }
        out
    }

    fn random_name(&mut self) -> String {
        loop {
            let n = format!("{} {}", FIRST_NAMES.choose(&mut self.rng).unwrap(), LAST_NAMES.choose(&mut self.rng).unwrap());
            if !self.people.iter().any(|p| p.name == n) {
                return n;
            }
        }
    }

    fn free_bed(&self, kinds: &[BuildingKind]) -> Option<(usize, usize, Pos)> {
        for kind in kinds {
            for b in self.map.buildings.iter().filter(|b| b.kind == *kind) {
                for (ui, unit) in b.units.iter().enumerate() {
                    for bed in unit {
                        if !self.people.iter().any(|p| p.alive && p.bed == Some(*bed)) {
                            return Some((b.id, ui, *bed));
                        }
                    }
                }
            }
        }
        None
    }

    pub fn job_holders(&self, j: usize) -> usize {
        self.people.iter().filter(|p| p.alive && p.job == Some(j)).count()
    }
    pub fn job_vacant(&self, j: usize) -> bool {
        self.job_holders(j) < self.jobs[j].slots as usize
    }

    pub fn assign_job(&mut self, i: usize, j: usize) {
        let kind = self.jobs[j].building;
        let spot_kind = self.jobs[j].spot;
        let Some(b) = self.map.first_of(kind) else { return };
        let taken: Vec<Pos> = self.people.iter().filter(|p| p.alive && p.id != i).filter_map(|p| p.work_spot).collect();
        let spots = &self.map.buildings[b].work_spots;
        let spot = spots
            .iter()
            .find(|(p, k)| *k == spot_kind && !taken.contains(p))
            .or_else(|| spots.iter().find(|(p, _)| !taken.contains(p)))
            .or_else(|| spots.first())
            .map(|(p, _)| *p);
        let p = &mut self.people[i];
        p.job = Some(j);
        p.workplace = Some(b);
        p.work_spot = spot;
        p.missed_shifts = 0;
        p.performance = 0;
        p.worked = 0;
        p.was_on_shift = false;
    }

    pub fn assign_home(&mut self, i: usize) {
        let tier = self.tier(i);
        let pref: &[BuildingKind] = match tier {
            0..=1 => &[BuildingKind::Tenement, BuildingKind::Apartments],
            2..=3 => &[BuildingKind::Apartments, BuildingKind::Tenement, BuildingKind::Condos],
            _ => &[BuildingKind::Condos, BuildingKind::Apartments],
        };
        if let Some((b, u, bed)) = self.free_bed(pref) {
            let p = &mut self.people[i];
            p.home = Some(b);
            p.unit = Some(u);
            p.bed = Some(bed);
        }
    }

    pub fn make_person(&mut self, job: Option<usize>, pos: Pos) -> usize {
        let id = self.people.len();
        let name = self.random_name();
        let age = self.rng.gen_range(19..72);
        let mut p = Person::new(id, name, age, pos);
        p.traits = self.random_traits();
        p.hunger = self.rng.gen_range(5.0..40.0);
        p.energy = self.rng.gen_range(60.0..95.0);
        p.stress = self.rng.gen_range(10.0..45.0);
        p.lonely = self.rng.gen_range(10.0..50.0);
        if p.has(Trait::Smoker) {
            p.inventory.push(ItemKind::Cigarettes);
            p.inventory.push(ItemKind::Lighter);
        }
        if p.has(Trait::Hothead) && self.rng.gen::<f32>() < 0.35 {
            p.inventory.push(ItemKind::Knife);
        }
        self.people.push(p);
        if let Some(j) = job {
            self.assign_job(id, j);
        }
        let tier = self.tier(id) as i32;
        let base = self.rng.gen_range(20..90);
        self.people[id].money = base * (1 + tier * tier * 2) + if job.is_none() { 0 } else { 30 };
        self.assign_home(id);
        if let Some(bed) = self.people[id].bed {
            self.people[id].pos = bed;
        }
        id
    }

    fn populate(&mut self, start: StartJob) {
        // Workers
        for j in 0..self.jobs.len() {
            let essential = matches!(self.jobs[j].role, Role::Cop | Role::Firefighter | Role::Medic);
            for s in 0..self.jobs[j].slots {
                if (essential && s == 0) || self.chance(0.82) {
                    self.make_person(Some(j), Pos::new(1, 1));
                }
            }
        }
        // The unemployed, the between-jobs, the retired, the "consultants".
        for _ in 0..7 {
            self.make_person(None, Pos::new(1, 1));
        }
        // Everyone without a bed lives in the park.
        let park = self.map.first_of(BuildingKind::Park);
        for i in 0..self.people.len() {
            if self.people[i].bed.is_none() {
                if let Some(pk) = park {
                    let seats = self.map.buildings[pk].seats.clone();
                    if let Some(s) = seats.choose(&mut self.rng) {
                        self.people[i].pos = *s;
                    }
                }
            }
        }

        // The player.
        let job = match start {
            StartJob::Random => {
                if self.chance(0.12) {
                    None
                } else {
                    Some(self.rng.gen_range(0..self.jobs.len()))
                }
            }
            StartJob::Unemployed => None,
            StartJob::Job(j) => Some(j),
        };
        // If the job is full, someone else just got let go. That's how it goes.
        if let Some(j) = job {
            if !self.job_vacant(j) {
                if let Some(victim) = self.people.iter().position(|p| p.job == Some(j)) {
                    let p = &mut self.people[victim];
                    p.job = None;
                    p.workplace = None;
                    p.work_spot = None;
                }
            }
        }
        let pid = self.make_person(job, Pos::new(1, 1));
        self.player = pid;
        {
            let p = &mut self.people[pid];
            p.is_player = true;
            p.inventory.push(ItemKind::Lottery);
            p.inventory.push(ItemKind::Snack);
            p.hunger = 30.0;
            p.energy = 85.0;
            p.stress = 35.0;
        }
        if self.people[pid].bed.is_none() {
            // No home. Start in the park, like the others who fell through.
            if let Some(pk) = park {
                let s = self.map.buildings[pk].seats[0];
                self.people[pid].pos = s;
            }
        }

        // Relationships: roommates, coworkers, and old grudges.
        let n = self.people.len();
        for a in 0..n {
            for b in 0..n {
                if a == b {
                    continue;
                }
                let same_unit = self.people[a].home.is_some()
                    && self.people[a].home == self.people[b].home
                    && self.people[a].unit == self.people[b].unit;
                let coworkers = self.people[a].workplace.is_some() && self.people[a].workplace == self.people[b].workplace;
                let mut o = 0;
                if same_unit {
                    o += self.rng.gen_range(-40..50);
                }
                if coworkers {
                    o += self.rng.gen_range(-25..30);
                }
                if o != 0 {
                    self.people[a].adjust_opinion(b, o);
                }
            }
        }
        for _ in 0..n / 4 {
            let a = self.rng.gen_range(0..n);
            let b = self.rng.gen_range(0..n);
            if a != b && a != pid {
                let o = self.rng.gen_range(-90..-45);
                self.people[a].adjust_opinion(b, o);
            }
        }
        for _ in 0..n / 5 {
            let a = self.rng.gen_range(0..n);
            let b = self.rng.gen_range(0..n);
            if a != b {
                let o = self.rng.gen_range(30..70);
                self.people[a].adjust_opinion(b, o);
                self.people[b].adjust_opinion(a, o / 2);
            }
        }
        // A couple of people know and quietly like the player.
        for _ in 0..3 {
            let a = self.rng.gen_range(0..n);
            if a != pid {
                let o = self.rng.gen_range(15..45);
                self.people[a].adjust_opinion(pid, o);
            }
        }
    }

    fn intro(&mut self) {
        let pid = self.player;
        let p = self.people[pid].clone();
        let job = self.job_name(pid).to_string();
        let home = match p.home {
            Some(h) => format!("in a room at {}", self.map.buildings[h].name),
            None => "on a park bench. You do not currently have a home".to_string(),
        };
        let traits: Vec<&str> = p.traits.iter().map(|t| t.name()).collect();
        self.say(None, &[pid], LogKind::Info, format!("Day 1. {}. You wake up {}.", self.weekday(), home));
        self.say(
            None,
            &[pid],
            LogKind::Normal,
            format!("You are {}, {}. Occupation: {}. People would describe you as {}.", p.name, p.age, job, traits.join(", ")),
        );
        let rent = match p.home {
            Some(h) => format!(" Rent is due Monday: ${}.", { let _ = h; self.rent_share(pid) }),
            None => String::new(),
        };
        self.say(None, &[pid], LogKind::Normal, format!("You have ${}.{}", p.money, rent));
        match self.minutes_until_shift(pid) {
            Some(m) if m < 24 * 60 => {
                let j = &self.jobs[p.job.unwrap()];
                let wp = &self.map.buildings[p.workplace.unwrap()].name;
                self.say(None, &[pid], LogKind::Normal, format!("Your shift at {} starts at {:02}:00.", wp, j.start));
            }
            _ => {}
        }
        self.say(None, &[pid], LogKind::Weird, "Somewhere under the city, something hums. It always has.");
        self.say(None, &[pid], LogKind::Info, "Press ? for help.");
    }

    pub fn rent_share(&self, i: usize) -> i32 {
        let p = &self.people[i];
        let (Some(h), Some(u)) = (p.home, p.unit) else { return 0 };
        let b = &self.map.buildings[h];
        let beds = b.units[u].len().max(1) as i32;
        b.kind.rent() / beds
    }

    // ------------------------------------------------------------------ tick

    pub fn step(&mut self) {
        self.turn += 1;
        let n = self.people.len();
        for i in 0..n {
            self.update_needs(i);
        }
        fire::step(self);
        for i in 0..n {
            self.update_shift(i);
        }
        for i in 0..n {
            if !self.people[i].is_player || self.autopilot {
                ai::think(self, i);
            }
        }
        self.player_step();
        let m = self.minute();
        if m % 60 == 0 {
            self.hourly();
        }
        if m == 0 {
            self.daily();
        }
        events::check_objectives(self);
        if let Some((_, until)) = self.phone_ringing {
            if self.turn >= until {
                self.phone_ringing = None;
            }
        }
        self.update_fov();
    }

    pub fn update_fov(&mut self) {
        let pos = self.people[self.player].pos;
        let r = if self.is_night() && !self.map.indoors(pos) { 9 } else { 22 };
        path::fov(&self.map, pos, r, &mut self.visible);
        for (s, v) in self.seen.iter_mut().zip(self.visible.iter()) {
            *s |= *v;
        }
    }

    fn update_needs(&mut self, i: usize) {
        let now = self.turn;
        let r: f32 = self.rng.gen();
        let p = &mut self.people[i];
        if !p.alive {
            return;
        }
        if p.jailed_until > now {
            // Jail: three grey meals a day and a bench to sleep on.
            p.hunger = (p.hunger - 0.05).max(10.0);
            p.energy = (p.energy + 0.1).min(100.0);
            p.stress = (p.stress + 0.01).min(100.0);
            return;
        }
        let out = p.unconscious_until > now;
        if p.sleeping || out {
            p.energy = (p.energy + 0.2).min(100.0);
            p.hunger += 0.04;
            // Rest heals, slowly, if you're fed.
            if now % 45 == 0 && p.hunger < 80.0 {
                if let Some(k) = (0..6).filter(|k| p.body.hp[*k] < p.body.max[*k]).min_by_key(|k| p.body.hp[*k] * 100 / p.body.max[*k]) {
                    p.body.hp[k] += 1;
                }
            }
        } else {
            p.energy -= 0.062 + p.drunk * 0.0005;
            p.hunger += 0.085;
        }
        p.hunger = p.hunger.min(100.0);
        p.lonely = (p.lonely + 0.035).min(100.0);
        p.drunk = (p.drunk - 0.06).max(0.0);
        let mut ds = -0.02;
        if p.hunger > 70.0 {
            ds += 0.03;
        }
        if p.energy < 20.0 {
            ds += 0.03;
        }
        if p.money < 0 {
            ds += 0.015;
        }
        if p.home.is_none() {
            ds += 0.015;
        }
        if p.lonely > 80.0 {
            ds += 0.02;
        }
        if p.body.hurt() {
            ds += 0.03;
        }
        p.stress = (p.stress + ds).clamp(0.0, 100.0);
        if p.hostile_until <= now {
            p.hostile_to = None;
        }
        let mut death: Option<&str> = None;
        if p.body.bleeding > 0 {
            p.body.blood -= p.body.bleeding as f32 * 0.07;
            if r < 0.005 {
                p.body.bleeding -= 1;
            }
            if p.body.blood <= 0.0 {
                death = Some("bled out");
            }
        } else if p.body.blood < 100.0 {
            p.body.blood = (p.body.blood + 0.01).min(100.0);
        }
        if p.hunger >= 100.0 && now % 90 == 0 {
            p.body.hp[TORSO] -= 1;
            if p.body.hp[TORSO] <= 0 {
                death = Some("starved, quietly");
            }
        }
        p.energy = p.energy.max(0.0);
        let collapse = p.energy <= 0.0 && !p.sleeping && !out;
        if collapse {
            p.unconscious_until = now + 150;
            p.intent = Intent::default();
            p.path.clear();
        }
        let pos = p.pos;
        if collapse {
            let n = self.cname(i);
            self.say(Some(pos), &[i], LogKind::Bad, format!("{} collapse{} from exhaustion.", n, if i == self.player { "" } else { "s" }));
        }
        if let Some(cause) = death {
            self.kill(i, cause.to_string(), None);
        }
    }

    fn update_shift(&mut self, i: usize) {
        let Some(j) = self.people[i].job else { return };
        if !self.people[i].alive {
            return;
        }
        let on = self.on_shift(i);
        let p = &self.people[i];
        let working = if p.is_player && !self.autopilot {
            self.busy == Some(Busy::Work)
        } else {
            p.intent.kind == IntentKind::Work && p.intent.started
        };
        if on && working {
            self.people[i].worked += 1;
        }
        if i == self.player && !on && self.on_shift_at(i, self.turn + 60) && !self.on_shift_at(i, self.turn + 59) {
            let wp = self.people[i].workplace.map(|b| self.map.buildings[b].name.clone()).unwrap_or_default();
            if self.busy == Some(Busy::Sleep) {
                self.say(None, &[i], LogKind::Bad, format!("Your alarm goes off. Your shift at {} starts in an hour.", wp));
                self.interrupt = true;
            } else {
                self.say(None, &[i], LogKind::Info, format!("Your shift at {} starts in an hour.", wp));
            }
        }
        if self.people[i].was_on_shift && !on {
            let job = self.jobs[j].clone();
            let worked = self.people[i].worked;
            let pl = i == self.player;
            if worked as f32 >= job.hours as f32 * 60.0 * 0.6 {
                let p = &mut self.people[i];
                p.money += job.wage;
                p.performance += 1;
                if p.has(Trait::Diligent) {
                    p.performance += 1;
                }
                p.missed_shifts = p.missed_shifts.saturating_sub(1);
                if pl {
                    self.stats.player_shifts += 1;
                    let quip = [
                        "It is already spoken for.",
                        "Nobody says thank you.",
                        "Your manager calls you 'buddy'. You are not buddies.",
                        "You feel slightly more replaceable than yesterday.",
                        "The clock seemed to run backwards for an hour in the middle there.",
                    ];
                    let q = quip.choose(&mut self.rng).unwrap();
                    self.say(None, &[i], LogKind::Good, format!("Shift over. ${} earned. {}", job.wage, q));
                }
            } else {
                let p = &mut self.people[i];
                p.missed_shifts += 1;
                p.performance -= 2;
                let missed = p.missed_shifts;
                if pl {
                    self.say(None, &[i], LogKind::Bad, format!("You missed your shift as {}. Strike {} of 3.", job.name, missed));
                }
                if missed >= 3 {
                    self.fire_from_job(i, "for missing too many shifts");
                }
            }
            self.people[i].worked = 0;
        }
        self.people[i].was_on_shift = self.on_shift(i);
    }

    pub fn fire_from_job(&mut self, i: usize, why: &str) {
        let name = self.job_name(i).to_string();
        let p = &mut self.people[i];
        p.job = None;
        p.workplace = None;
        p.work_spot = None;
        p.stress = (p.stress + 25.0).min(100.0);
        if i == self.player {
            self.say(None, &[i], LogKind::Danger, format!("You've been fired as {} {}. The email has a smiley face in the signature.", name, why));
        } else {
            let pos = self.people[i].pos;
            let n = self.people[i].name.clone();
            self.say(Some(pos), &[i], LogKind::Info, format!("{} has been let go as {} {}.", n, name, why));
        }
    }

    fn player_step(&mut self) {
        let pid = self.player;
        if self.autopilot {
            return;
        }
        let now = self.turn;
        if !self.people[pid].alive {
            self.busy = None;
            return;
        }
        if self.interrupt && self.busy.is_some() && self.busy != Some(Busy::Jail) {
            if self.busy == Some(Busy::Sleep) {
                self.say(None, &[pid], LogKind::Bad, "Something wakes you.");
            }
            self.busy = None;
            self.people[pid].sleeping = false;
        }
        self.interrupt = false;
        match self.busy {
            Some(Busy::Sleep) => {
                let p = &self.people[pid];
                if p.energy >= 98.0 || (p.hunger > 92.0) {
                    self.busy = None;
                    self.people[pid].sleeping = false;
                    let lines = [
                        "You wake up. The ceiling has a new stain. It looks like your father.",
                        "You wake up. You dreamt of a hallway that went on forever. You were late for it.",
                        "You wake up tired, which is a kind of skill.",
                        "You wake up. The hum is still there. So are you.",
                        "You wake up and, for three seconds, forget who you are. Then it comes back.",
                    ];
                    let l = lines.choose(&mut self.rng).unwrap().to_string();
                    self.say(None, &[pid], LogKind::Normal, l);
                }
            }
            Some(Busy::Work) => {
                if !self.on_shift(pid) {
                    self.busy = None;
                }
            }
            Some(Busy::Wait(until)) => {
                if now >= until {
                    self.busy = None;
                }
            }
            Some(Busy::Jail) => {
                if self.people[pid].jailed_until <= now {
                    self.busy = None;
                }
            }
            None => {}
        }
        if self.people[pid].jailed_until > 0 && self.people[pid].jailed_until <= now {
            crate::combat::release(self, pid);
        }
    }

    fn hourly(&mut self) {
        let h = self.hour();
        events::hourly(self);
        // Crimes nobody solved go cold.
        let now = self.turn;
        for c in self.crimes.iter_mut() {
            if !c.resolved && now - c.turn > 3 * MINUTES_PER_DAY {
                c.resolved = true;
            }
        }
        if h == 8 {
            self.newcomers();
        }
        if h == 10 {
            self.rebuild();
        }
    }

    /// Contractors eventually show up. The city always patches itself, badly.
    fn rebuild(&mut self) {
        for b in 0..self.map.buildings.len() {
            let bl = self.map.buildings[b].clone();
            let mut damaged = false;
            let mut burning = false;
            for y in bl.y..bl.y + bl.h {
                for x in bl.x..bl.x + bl.w {
                    let k = self.map.idx(Pos::new(x, y));
                    let t = self.map.tiles[k];
                    burning |= t.fire > 0;
                    let o = self.map.original[k];
                    damaged |= t.terrain != o.terrain || t.furn != o.furn;
                }
            }
            if !damaged || burning || !self.chance(0.35) {
                continue;
            }
            for y in bl.y..bl.y + bl.h {
                for x in bl.x..bl.x + bl.w {
                    let k = self.map.idx(Pos::new(x, y));
                    let o = self.map.original[k];
                    let t = &mut self.map.tiles[k];
                    t.terrain = o.terrain;
                    t.furn = o.furn;
                    t.fuel = o.fuel;
                }
            }
            let lines = [
                "The new paint smells exactly like the old paint.",
                "Nobody mentions the fire. There is a new fire extinguisher, still in its box.",
                "The insurance paid for it. Somebody's rent will go up.",
            ];
            let l = lines.choose(&mut self.rng).unwrap();
            self.say(None, &[], LogKind::Info, format!("Contractors finish patching up {}. {}", bl.name, l));
        }
    }

    fn daily(&mut self) {
        let day = self.day();
        // Rent: every Monday.
        if day % 7 == 0 && day > 0 {
            self.collect_rent();
        }
        // Promotions on Saturday night.
        if day % 7 == 6 {
            self.promotions();
        }
    }

    fn collect_rent(&mut self) {
        let pid = self.player;
        for i in 0..self.people.len() {
            if !self.people[i].alive || self.people[i].home.is_none() {
                continue;
            }
            let share = self.rent_share(i);
            let p = &mut self.people[i];
            let due = share + p.rent_owed;
            if p.money >= due {
                p.money -= due;
                p.rent_owed = 0;
                if i == pid {
                    self.say(None, &[i], LogKind::Bad, format!("Rent day. ${} leaves your account like it was never yours.", due));
                }
            } else {
                let paid = p.money.max(0);
                p.money -= paid;
                p.rent_owed = due - paid;
                p.stress = (p.stress + 20.0).min(100.0);
                let owed = p.rent_owed;
                if owed as f32 > share as f32 * 1.5 {
                    self.evict(i);
                } else if i == pid {
                    self.say(
                        None,
                        &[i],
                        LogKind::Danger,
                        format!("Rent day. You're ${} short. A note appears under your door. It just says 'SOON'.", owed),
                    );
                }
            }
        }
    }

    pub fn evict(&mut self, i: usize) {
        let home = self.people[i].home.map(|h| self.map.buildings[h].name.clone()).unwrap_or_default();
        let p = &mut self.people[i];
        p.home = None;
        p.unit = None;
        p.bed = None;
        p.rent_owed = 0;
        p.stress = (p.stress + 30.0).min(100.0);
        if i == self.player {
            self.say(None, &[i], LogKind::Danger, format!("You have been evicted from {}. Your things are on the curb. Some of them are already gone.", home));
        } else {
            let n = self.people[i].name.clone();
            self.say(None, &[], LogKind::Info, format!("{} has been evicted from {}.", n, home));
        }
    }

    fn promotions(&mut self) {
        for i in 0..self.people.len() {
            let Some(j) = self.people[i].job else { continue };
            if !self.people[i].alive || self.people[i].performance < 9 {
                continue;
            }
            let Some(next) = self.jobs[j].promotes_to.clone() else { continue };
            let Some(nj) = self.jobs.iter().position(|d| d.name == next) else { continue };
            if !self.job_vacant(nj) {
                continue;
            }
            self.assign_job(i, nj);
            self.stats.promotions += 1;
            if i == self.player {
                self.say(None, &[i], LogKind::Good, format!("You've been promoted to {}. Your new title has more words in it. Your life does not.", next));
            } else {
                let n = self.people[i].name.clone();
                self.say(None, &[], LogKind::Info, format!("{} was promoted to {}.", n, next));
            }
        }
    }

    fn newcomers(&mut self) {
        let alive = self.people.iter().filter(|p| p.alive).count();
        if alive >= self.target_population || !self.chance(0.6) {
            return;
        }
        let vacancies: Vec<usize> = (0..self.jobs.len()).filter(|j| self.job_vacant(*j)).collect();
        let job = if self.chance(0.6) { vacancies.choose(&mut self.rng).copied() } else { None };
        let edge = Pos::new(1, self.rng.gen_range(1..self.map.h - 1));
        let i = self.make_person(job, edge);
        if self.people[i].bed.is_none() {
            // No room at the inn. They leave again; the city wasn't for them.
            self.people[i].alive = false;
            self.people[i].cause_of_death = Some("left town".into());
            return;
        }
        self.people[i].pos = edge;
        let home = self.map.buildings[self.people[i].home.unwrap()].name.clone();
        let n = self.people[i].name.clone();
        self.say(
            None,
            &[],
            LogKind::Info,
            format!("A moving van idles outside {}. {} moves in. They don't ask what happened to the last tenant.", home, n),
        );
    }

    // ------------------------------------------------------------------ life & crime

    pub fn kill(&mut self, i: usize, cause: String, killer: Option<usize>) {
        if !self.people[i].alive {
            return;
        }
        let pos = self.people[i].pos;
        {
            let p = &mut self.people[i];
            p.alive = false;
            p.sleeping = false;
            p.cause_of_death = Some(cause.clone());
            p.died_turn = self.turn;
            p.path.clear();
        }
        self.stats.deaths += 1;
        let n = self.cname(i);
        if i == self.player {
            self.say(Some(pos), &[i], LogKind::Danger, format!("You die. ({})", cause));
        } else {
            self.say(Some(pos), &[i], LogKind::Danger, format!("{} dies. ({})", n, cause));
        }
        if let Some(k) = killer {
            self.people[k].kills += 1;
            self.crime(CrimeKind::Murder, k, Some(i), pos);
        }
        // Everyone who could see it remembers.
        let now = self.turn;
        for j in 0..self.people.len() {
            if j == i || !self.people[j].awake(now) {
                continue;
            }
            let wp = self.people[j].pos;
            if wp.dist(pos) <= 8 && path::los(&self.map, wp, pos) {
                let p = &mut self.people[j];
                p.stress = (p.stress + 25.0).min(100.0);
                p.remember(now, MemKind::SawCorpse, killer, Some(i));
            }
        }
    }

    pub fn crime(&mut self, kind: CrimeKind, perp: usize, victim: Option<usize>, pos: Pos) {
        let now = self.turn;
        if self
            .crimes
            .iter()
            .any(|c| c.perp == perp && c.victim == victim && c.kind == kind && !c.resolved && now - c.turn < 60)
        {
            return;
        }
        if kind == CrimeKind::Assault {
            self.stats.fights += 1;
        }
        self.crimes.push(Crime { kind, perp, victim, pos, turn: now, reported: false, resolved: false });
        let ci = self.crimes.len() - 1;
        self.witness(ci);
    }

    fn witness(&mut self, ci: usize) {
        let c = self.crimes[ci].clone();
        let now = self.turn;
        let sev = c.kind.severity();
        let mut reporter = None;
        for j in 0..self.people.len() {
            if j == c.perp || !self.people[j].awake(now) {
                continue;
            }
            let wp = self.people[j].pos;
            let is_victim = Some(j) == c.victim;
            if !is_victim && (wp.dist(c.pos) > 9 || !path::los(&self.map, wp, c.pos)) {
                continue;
            }
            let hates_victim = c.victim.map(|v| self.people[j].opinion(v) < -40).unwrap_or(false);
            let likes_victim = c.victim.map(|v| self.people[j].opinion(v) > 35).unwrap_or(false);
            let mem = match (c.kind, is_victim) {
                (CrimeKind::Assault, true) => MemKind::Attacked,
                (CrimeKind::Assault, false) => MemKind::SawAssault,
                (CrimeKind::Murder, _) => MemKind::SawMurder,
                (CrimeKind::Theft | CrimeKind::Pickpocket, true) => MemKind::Robbed,
                (CrimeKind::Theft | CrimeKind::Pickpocket, false) => MemKind::SawTheft,
                (CrimeKind::Arson, _) => MemKind::SawFire,
            };
            let r: f32 = self.rng.gen();
            let cop = self.on_duty(j, Role::Cop);
            let p = &mut self.people[j];
            p.adjust_opinion(c.perp, if hates_victim { -sev * 2 } else { -sev * 9 });
            p.remember(now, mem, Some(c.perp), c.victim);
            p.stress = (p.stress + sev as f32 * 3.0).min(100.0);
            if j != self.player || self.autopilot {
                if cop {
                    p.intent = Intent { kind: IntentKind::Chase(c.perp), ..Default::default() };
                } else if !is_victim
                    && matches!(c.kind, CrimeKind::Assault | CrimeKind::Murder)
                    && (p.has(Trait::Brave) || likes_victim)
                    && !p.has(Trait::Coward)
                    && r < 0.5
                {
                    p.hostile_to = Some(c.perp);
                    p.hostile_until = now + 40;
                } else if !is_victim && (p.has(Trait::Coward) || c.kind == CrimeKind::Murder) {
                    p.intent = Intent { kind: IntentKind::Flee, target: c.pos, until: now + 20, ..Default::default() };
                    p.path.clear();
                }
            }
            if reporter.is_none() && j != self.player && !hates_victim {
                reporter = Some(j);
            }
        }
        if let Some(r) = reporter {
            self.crimes[ci].reported = true;
            let rp = self.people[r].pos;
            let n = self.people[r].name.clone();
            let pn = self.name(c.perp);
            let what = c.kind.name();
            self.say(Some(rp), &[r], LogKind::Bad, format!("{} calls the police about {} ({}).", n, pn, what));
        }
    }

    /// The most serious open, reported crime whose perpetrator is still free.
    pub fn wanted(&self) -> Option<usize> {
        let now = self.turn;
        self.crimes
            .iter()
            .filter(|c| c.reported && !c.resolved)
            .filter(|c| {
                let p = &self.people[c.perp];
                p.alive && p.jailed_until <= now
            })
            .max_by_key(|c| c.kind.severity())
            .map(|c| c.perp)
    }

    pub fn heat(&self, i: usize) -> i32 {
        self.crimes.iter().filter(|c| c.perp == i && c.reported && !c.resolved).map(|c| c.kind.severity()).sum()
    }

    pub fn person_at(&self, p: Pos) -> Option<usize> {
        // Living people first, then the dead.
        self.people
            .iter()
            .filter(|q| q.pos == p && q.alive && q.jailed_until <= self.turn || q.pos == p && q.alive && q.is_player)
            .map(|q| q.id)
            .next()
            .or_else(|| self.people.iter().find(|q| q.pos == p && !q.alive && q.cause_of_death.as_deref() != Some("left town")).map(|q| q.id))
    }
    pub fn living_at(&self, p: Pos) -> Option<usize> {
        self.people.iter().find(|q| q.pos == p && q.alive && q.id != self.player).map(|q| q.id)
    }

    pub fn is_floorish(&self, p: Pos) -> bool {
        self.map.in_bounds(p) && !matches!(self.map.get(p).terrain, Terrain::Wall | Terrain::Window)
    }
}
