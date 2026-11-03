//! People: bodies, needs, traits, memories and opinions.

use crate::map::Pos;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trait {
    Kind,
    Petty,
    Hothead,
    Coward,
    Brave,
    Drunkard,
    Smoker,
    Paranoid,
    Gossip,
    Greedy,
    Lazy,
    Diligent,
    Weird,
    Charming,
}

impl Trait {
    pub const ALL: [Trait; 14] = [
        Trait::Kind,
        Trait::Petty,
        Trait::Hothead,
        Trait::Coward,
        Trait::Brave,
        Trait::Drunkard,
        Trait::Smoker,
        Trait::Paranoid,
        Trait::Gossip,
        Trait::Greedy,
        Trait::Lazy,
        Trait::Diligent,
        Trait::Weird,
        Trait::Charming,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Trait::Kind => "kind",
            Trait::Petty => "petty",
            Trait::Hothead => "hot-headed",
            Trait::Coward => "cowardly",
            Trait::Brave => "brave",
            Trait::Drunkard => "a drinker",
            Trait::Smoker => "a smoker",
            Trait::Paranoid => "paranoid",
            Trait::Gossip => "a gossip",
            Trait::Greedy => "greedy",
            Trait::Lazy => "lazy",
            Trait::Diligent => "diligent",
            Trait::Weird => "strange",
            Trait::Charming => "charming",
        }
    }
    pub fn conflicts(self, o: Trait) -> bool {
        use Trait::*;
        matches!((self, o), (Coward, Brave) | (Brave, Coward) | (Lazy, Diligent) | (Diligent, Lazy) | (Kind, Petty) | (Petty, Kind))
    }
}

pub const PART_NAMES: [&str; 6] = ["head", "torso", "left arm", "right arm", "left leg", "right leg"];
pub const HEAD: usize = 0;
pub const TORSO: usize = 1;

#[derive(Clone, Debug)]
pub struct Body {
    pub hp: [i32; 6],
    pub max: [i32; 6],
    pub bleeding: i32,
    pub blood: f32,
}

impl Default for Body {
    fn default() -> Self {
        let max = [12, 24, 12, 12, 14, 14];
        Body { hp: max, max, bleeding: 0, blood: 100.0 }
    }
}

impl Body {
    pub fn health_frac(&self) -> f32 {
        let t: i32 = self.hp.iter().map(|h| (*h).max(0)).sum();
        let m: i32 = self.max.iter().sum();
        t as f32 / m as f32 * (self.blood / 100.0).min(1.0)
    }
    pub fn hurt(&self) -> bool {
        self.health_frac() < 0.75 || self.bleeding > 0
    }
    pub fn legs_ok(&self) -> bool {
        self.hp[4] > 0 || self.hp[5] > 0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemKind {
    Insulted,
    Complimented,
    Attacked,
    SawAssault,
    SawMurder,
    SawFire,
    SawTheft,
    Robbed,
    HeardRumor,
    SawCorpse,
    Arrested,
}

#[derive(Clone, Debug)]
pub struct Memory {
    pub turn: u64,
    pub kind: MemKind,
    /// The person the memory is mostly about (the perpetrator, the insulter...).
    pub about: Option<usize>,
    pub victim: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntentKind {
    None,
    Work,
    Sleep,
    Eat,
    EatHome,
    Drink,
    Socialize,
    Clinic,
    Shop,
    Steal,
    Scavenge,
    Flee,
    Fight(usize),
    Chase(usize),
    Extinguish,
    Wander,
    Arson,
    Loiter,
}

#[derive(Clone, Copy, Debug)]
pub struct Intent {
    pub kind: IntentKind,
    pub target: Pos,
    pub building: Option<usize>,
    /// Set when the person arrived and started the activity.
    pub started: bool,
    pub until: u64,
}

impl Default for Intent {
    fn default() -> Self {
        Intent { kind: IntentKind::None, target: Pos::default(), building: None, started: false, until: 0 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemKind {
    Snack,
    Beer,
    Cigarettes,
    Lighter,
    Knife,
    Bandage,
    Painkillers,
    Lottery,
}

impl ItemKind {
    pub fn name(self) -> &'static str {
        match self {
            ItemKind::Snack => "sad sandwich",
            ItemKind::Beer => "can of beer",
            ItemKind::Cigarettes => "pack of cigarettes",
            ItemKind::Lighter => "lighter",
            ItemKind::Knife => "kitchen knife",
            ItemKind::Bandage => "bandage",
            ItemKind::Painkillers => "painkillers",
            ItemKind::Lottery => "scratch-off ticket",
        }
    }
    pub fn price(self) -> i32 {
        match self {
            ItemKind::Snack => 5,
            ItemKind::Beer => 4,
            ItemKind::Cigarettes => 9,
            ItemKind::Lighter => 2,
            ItemKind::Knife => 15,
            ItemKind::Bandage => 6,
            ItemKind::Painkillers => 8,
            ItemKind::Lottery => 3,
        }
    }
    pub const SHOP: [ItemKind; 8] = [
        ItemKind::Snack,
        ItemKind::Beer,
        ItemKind::Cigarettes,
        ItemKind::Lighter,
        ItemKind::Bandage,
        ItemKind::Painkillers,
        ItemKind::Lottery,
        ItemKind::Knife,
    ];
}

#[derive(Clone, Debug)]
pub struct Person {
    pub id: usize,
    pub name: String,
    pub age: u32,
    pub alive: bool,
    pub is_player: bool,
    pub pos: Pos,
    pub job: Option<usize>,
    pub workplace: Option<usize>,
    pub work_spot: Option<Pos>,
    pub home: Option<usize>,
    pub unit: Option<usize>,
    pub bed: Option<Pos>,
    pub money: i32,
    pub rent_owed: i32,
    pub hunger: f32,
    pub energy: f32,
    pub stress: f32,
    pub lonely: f32,
    pub drunk: f32,
    pub body: Body,
    pub traits: Vec<Trait>,
    pub opinions: BTreeMap<usize, i32>,
    pub memories: Vec<Memory>,
    pub intent: Intent,
    pub path: Vec<Pos>,
    pub path_goal: Pos,
    pub hostile_to: Option<usize>,
    pub hostile_until: u64,
    pub jailed_until: u64,
    pub unconscious_until: u64,
    pub record: u32,
    pub performance: i32,
    pub missed_shifts: u32,
    pub worked: u32,
    pub was_on_shift: bool,
    pub inventory: Vec<ItemKind>,
    pub hum: bool,
    pub last_chat: u64,
    pub last_scream: u64,
    pub last_clinic: u64,
    pub cause_of_death: Option<String>,
    pub died_turn: u64,
    pub kills: u32,
    pub sleeping: bool,
}

impl Person {
    pub fn new(id: usize, name: String, age: u32, pos: Pos) -> Self {
        Person {
            id,
            name,
            age,
            alive: true,
            is_player: false,
            pos,
            job: None,
            workplace: None,
            work_spot: None,
            home: None,
            unit: None,
            bed: None,
            money: 0,
            rent_owed: 0,
            hunger: 20.0,
            energy: 80.0,
            stress: 20.0,
            lonely: 20.0,
            drunk: 0.0,
            body: Body::default(),
            traits: vec![],
            opinions: BTreeMap::new(),
            memories: vec![],
            intent: Intent::default(),
            path: vec![],
            path_goal: Pos::default(),
            hostile_to: None,
            hostile_until: 0,
            jailed_until: 0,
            unconscious_until: 0,
            record: 0,
            performance: 0,
            missed_shifts: 0,
            worked: 0,
            was_on_shift: false,
            inventory: vec![],
            hum: false,
            last_chat: 0,
            last_scream: 0,
            last_clinic: 0,
            cause_of_death: None,
            died_turn: 0,
            kills: 0,
            sleeping: false,
        }
    }
    pub fn has(&self, t: Trait) -> bool {
        self.traits.contains(&t)
    }
    pub fn opinion(&self, of: usize) -> i32 {
        *self.opinions.get(&of).unwrap_or(&0)
    }
    pub fn adjust_opinion(&mut self, of: usize, delta: i32) {
        if of == self.id {
            return;
        }
        let e = self.opinions.entry(of).or_insert(0);
        *e = (*e + delta).clamp(-100, 100);
    }
    pub fn remember(&mut self, turn: u64, kind: MemKind, about: Option<usize>, victim: Option<usize>) {
        self.memories.push(Memory { turn, kind, about, victim });
        if self.memories.len() > 24 {
            self.memories.remove(0);
        }
    }
    pub fn awake(&self, now: u64) -> bool {
        self.alive && self.unconscious_until <= now && !self.sleeping
    }
    pub fn can_act(&self, now: u64) -> bool {
        self.alive && self.unconscious_until <= now && self.jailed_until <= now
    }
    pub fn first_name(&self) -> &str {
        self.name.split(' ').next().unwrap_or(&self.name)
    }
    pub fn weapon(&self) -> bool {
        self.inventory.contains(&ItemKind::Knife)
    }
}

pub fn opinion_word(o: i32) -> &'static str {
    match o {
        i32::MIN..=-70 => "despises",
        -69..=-35 => "hates",
        -34..=-10 => "dislikes",
        -9..=9 => "is indifferent to",
        10..=34 => "likes",
        35..=69 => "is fond of",
        _ => "adores",
    }
}

pub const FIRST_NAMES: [&str; 60] = [
    "Ada", "Bram", "Cleo", "Dmitri", "Edna", "Felix", "Gus", "Hester", "Ivo", "June", "Karl", "Lorna", "Milo", "Nadia", "Otto",
    "Pearl", "Quinn", "Rosa", "Sal", "Tova", "Ugo", "Vera", "Wade", "Xenia", "Yuri", "Zelda", "Abe", "Bettie", "Cyrus", "Dot",
    "Emil", "Fay", "Gord", "Hana", "Igor", "Jolene", "Kip", "Lev", "Mabel", "Ned", "Opal", "Percy", "Rhoda", "Stan", "Tilda",
    "Vic", "Wanda", "Arlo", "Bev", "Clint", "Dolores", "Earl", "Flo", "Grigor", "Hal", "Inez", "Jasper", "Kat", "Lou", "Moe",
];

pub const LAST_NAMES: [&str; 40] = [
    "Grimsby", "Holloway", "Kask", "Lumm", "Moss", "Nettles", "Okafor", "Petrov", "Quill", "Rook", "Sallow", "Tamm", "Underhill",
    "Vance", "Wick", "Yarrow", "Abernathy", "Bleak", "Crane", "Dunmore", "Esposito", "Farrow", "Gaunt", "Hobb", "Ives", "Jarvis",
    "Kowalski", "Lark", "Murk", "Nunez", "Ostrowski", "Pike", "Rasko", "Stubbs", "Thorne", "Ulrich", "Voss", "Wren", "Zane", "Mold",
];
