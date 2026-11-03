//! The city grid: terrain, furniture, fire, and the buildings laid on top.

use rand::seq::SliceRandom;
use rand::Rng;
use rand_pcg::Pcg64;
use serde::Deserialize;

pub const LOT_W: i32 = 28;
pub const LOT_H: i32 = 17;
pub const STREET: i32 = 3;
pub const LOTS_X: i32 = 4;
pub const LOTS_Y: i32 = 3;
pub const MAP_W: i32 = LOTS_X * (LOT_W + STREET) + STREET;
pub const MAP_H: i32 = LOTS_Y * (LOT_H + STREET) + STREET;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Pos {
    pub const fn new(x: i32, y: i32) -> Self {
        Pos { x, y }
    }
    /// Chebyshev distance (diagonal moves cost the same as straight ones).
    pub fn dist(self, o: Pos) -> i32 {
        (self.x - o.x).abs().max((self.y - o.y).abs())
    }
    pub fn add(self, dx: i32, dy: i32) -> Pos {
        Pos::new(self.x + dx, self.y + dy)
    }
    pub fn adjacent(self, o: Pos) -> bool {
        self.dist(o) == 1
    }
}

pub const DIRS8: [(i32, i32); 8] = [(0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Terrain {
    Road,
    Sidewalk,
    Grass,
    Wood,
    Carpet,
    Tile,
    Concrete,
    Wall,
    Door,
    Window,
    Ash,
}

impl Terrain {
    pub fn walkable(self) -> bool {
        !matches!(self, Terrain::Wall | Terrain::Window)
    }
    pub fn transparent(self) -> bool {
        !matches!(self, Terrain::Wall | Terrain::Door)
    }
    pub fn fuel(self) -> u8 {
        match self {
            Terrain::Wood => 14,
            Terrain::Carpet => 18,
            Terrain::Door => 25,
            Terrain::Grass => 3,
            _ => 0,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Terrain::Road => "road",
            Terrain::Sidewalk => "sidewalk",
            Terrain::Grass => "dead grass",
            Terrain::Wood => "wooden floor",
            Terrain::Carpet => "stained carpet",
            Terrain::Tile => "tiled floor",
            Terrain::Concrete => "concrete floor",
            Terrain::Wall => "wall",
            Terrain::Door => "door",
            Terrain::Window => "window",
            Terrain::Ash => "ash",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Furniture {
    None,
    Bed,
    Table,
    Chair,
    Counter,
    Desk,
    Stove,
    Shelf,
    Bench,
    Tree,
    BarCounter,
    Register,
    Toilet,
    Crate,
    Washer,
    Payphone,
    Trash,
    Sofa,
}

impl Furniture {
    pub fn walkable(self) -> bool {
        matches!(
            self,
            Furniture::None | Furniture::Bed | Furniture::Chair | Furniture::Bench | Furniture::Toilet | Furniture::Sofa | Furniture::Trash
        )
    }
    pub fn transparent(self) -> bool {
        !matches!(self, Furniture::Shelf | Furniture::Tree | Furniture::Crate)
    }
    pub fn fuel(self) -> u8 {
        match self {
            Furniture::Bed | Furniture::Sofa => 60,
            Furniture::Chair => 25,
            Furniture::Table | Furniture::Desk => 40,
            Furniture::Shelf | Furniture::Crate => 45,
            Furniture::Bench | Furniture::Counter => 30,
            Furniture::BarCounter => 50,
            Furniture::Tree => 50,
            Furniture::Trash => 35,
            _ => 0,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Furniture::None => "",
            Furniture::Bed => "bed",
            Furniture::Table => "table",
            Furniture::Chair => "chair",
            Furniture::Counter => "counter",
            Furniture::Desk => "desk",
            Furniture::Stove => "stove",
            Furniture::Shelf => "shelf",
            Furniture::Bench => "bench",
            Furniture::Tree => "tired tree",
            Furniture::BarCounter => "bar counter",
            Furniture::Register => "cash register",
            Furniture::Toilet => "toilet",
            Furniture::Crate => "crate",
            Furniture::Washer => "washing machine",
            Furniture::Payphone => "payphone",
            Furniture::Trash => "trash can",
            Furniture::Sofa => "sofa",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Tile {
    pub terrain: Terrain,
    pub furn: Furniture,
    pub fuel: u8,
    pub fire: u8,
    pub building: u8, // 255 = none
}

pub const NO_BUILDING: u8 = 255;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Hash)]
pub enum BuildingKind {
    Tenement,
    Apartments,
    Condos,
    Diner,
    Office,
    Clinic,
    CivicStation,
    Bar,
    Store,
    Warehouse,
    Park,
}

impl BuildingKind {
    pub fn is_home(self) -> bool {
        matches!(self, BuildingKind::Tenement | BuildingKind::Apartments | BuildingKind::Condos)
    }
    pub fn rent(self) -> i32 {
        match self {
            BuildingKind::Tenement => 140,
            BuildingKind::Apartments => 320,
            BuildingKind::Condos => 900,
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Default)]
pub enum SpotKind {
    #[default]
    Floor,
    Desk,
    Boss,
    Kitchen,
    Counter,
    Door,
}

#[derive(Clone, Debug)]
pub struct Building {
    pub id: usize,
    pub kind: BuildingKind,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub entrances: Vec<Pos>,
    /// Home units: each unit is a list of beds. Roommates share a unit.
    pub units: Vec<Vec<Pos>>,
    pub work_spots: Vec<(Pos, SpotKind)>,
    /// Where customers sit / stand.
    pub seats: Vec<Pos>,
    /// Where customers stand to be served (in front of a counter / register).
    pub service: Vec<Pos>,
    /// Clinic beds, jail cells.
    pub beds: Vec<Pos>,
    pub cells: Vec<Pos>,
    pub register: Option<Pos>,
    pub till: i32,
    pub burned: u32,
}

impl Building {
    pub fn contains(&self, p: Pos) -> bool {
        p.x >= self.x && p.x < self.x + self.w && p.y >= self.y && p.y < self.y + self.h
    }
    pub fn open(&self, hour: u32) -> bool {
        match self.kind {
            BuildingKind::Diner => (6..23).contains(&hour),
            BuildingKind::Bar => hour >= 16 || hour < 3,
            BuildingKind::Store => (6..24).contains(&hour),
            BuildingKind::Clinic => true,
            _ => true,
        }
    }
}

pub struct Map {
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub payphones: Vec<Pos>,
    /// The city as built, for repairs.
    pub original: Vec<Tile>,
}

impl Map {
    pub fn idx(&self, p: Pos) -> usize {
        (p.y * self.w + p.x) as usize
    }
    pub fn in_bounds(&self, p: Pos) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.w && p.y < self.h
    }
    pub fn get(&self, p: Pos) -> &Tile {
        &self.tiles[self.idx(p)]
    }
    pub fn get_mut(&mut self, p: Pos) -> &mut Tile {
        let i = self.idx(p);
        &mut self.tiles[i]
    }
    pub fn walkable(&self, p: Pos) -> bool {
        self.in_bounds(p) && {
            let t = self.get(p);
            t.terrain.walkable() && t.furn.walkable()
        }
    }
    pub fn transparent(&self, p: Pos) -> bool {
        self.in_bounds(p) && {
            let t = self.get(p);
            t.terrain.transparent() && t.furn.transparent()
        }
    }
    pub fn building_at(&self, p: Pos) -> Option<usize> {
        if !self.in_bounds(p) {
            return None;
        }
        let b = self.get(p).building;
        (b != NO_BUILDING).then_some(b as usize)
    }
    pub fn indoors(&self, p: Pos) -> bool {
        self.building_at(p).map(|b| self.buildings[b].kind != BuildingKind::Park).unwrap_or(false)
    }
    pub fn first_of(&self, kind: BuildingKind) -> Option<usize> {
        self.buildings.iter().position(|b| b.kind == kind)
    }
    pub fn street_name(&self, p: Pos) -> &'static str {
        const AVES: [&str; 5] = ["Grey Ave", "Lowell Ave", "Third Ave", "Marrow Ave", "Last Ave"];
        const STS: [&str; 4] = ["Penance St", "Cobb St", "Weir St", "Sallow St"];
        let col = p.x / (LOT_W + STREET);
        let row = p.y / (LOT_H + STREET);
        if p.y % (LOT_H + STREET) < STREET {
            STS[(row as usize).min(3)]
        } else {
            AVES[(col as usize).min(4)]
        }
    }

    fn set(&mut self, p: Pos, t: Terrain) {
        let tile = self.get_mut(p);
        tile.terrain = t;
    }
    fn furn(&mut self, p: Pos, f: Furniture) {
        self.get_mut(p).furn = f;
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, t: Terrain) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(Pos::new(xx, yy), t);
            }
        }
    }
    /// Walls on the border, floor inside.
    fn room(&mut self, x: i32, y: i32, w: i32, h: i32, floor: Terrain) {
        for yy in y..y + h {
            for xx in x..x + w {
                let edge = xx == x || yy == y || xx == x + w - 1 || yy == y + h - 1;
                self.set(Pos::new(xx, yy), if edge { Terrain::Wall } else { floor });
            }
        }
    }
    fn windows(&mut self, b: &Building) {
        for xx in b.x + 2..b.x + b.w - 2 {
            if (xx - b.x) % 4 == 2 {
                for yy in [b.y, b.y + b.h - 1] {
                    let p = Pos::new(xx, yy);
                    if self.get(p).terrain == Terrain::Wall && !self.near_door(p) {
                        self.set(p, Terrain::Window);
                    }
                }
            }
        }
    }
    fn near_door(&self, p: Pos) -> bool {
        DIRS8.iter().any(|(dx, dy)| {
            let q = p.add(*dx, *dy);
            self.in_bounds(q) && self.get(q).terrain == Terrain::Door
        })
    }
}

fn building_name(kind: BuildingKind, rng: &mut Pcg64) -> String {
    let pick = |rng: &mut Pcg64, xs: &[&str]| xs.choose(rng).unwrap().to_string();
    match kind {
        BuildingKind::Tenement => pick(rng, &["The Vesper Arms", "Cobb Street Tenements", "Halfway House", "The Gristle Block"]),
        BuildingKind::Apartments => pick(rng, &["Maple Court Apartments", "The Meridian", "Sunnyvale Units"]),
        BuildingKind::Condos => pick(rng, &["The Aurelian", "Skyline Residences", "One Grey Plaza"]),
        BuildingKind::Diner => pick(rng, &["Dot's Diner", "The Greasy Halo", "Eat Here Diner"]),
        BuildingKind::Office => pick(rng, &["Lattice Holdings Tower", "Moribund & Partners", "Synergex Plaza"]),
        BuildingKind::Clinic => pick(rng, &["St. Adequate Clinic", "MedFast Urgent Care"]),
        BuildingKind::CivicStation => "Precinct 9 & Firehouse".to_string(),
        BuildingKind::Bar => pick(rng, &["The Low Tide", "Last Call", "The Drowned Rat"]),
        BuildingKind::Store => pick(rng, &["Quik-E Mart", "Corner Store 24/7-ish", "Lucky Mart"]),
        BuildingKind::Warehouse => pick(rng, &["Dunmore Logistics", "Unit 9 Storage", "Crate & Despair Co."]),
        BuildingKind::Park => pick(rng, &["Memorial Park", "Pigeon Square"]),
    }
}

pub fn generate(rng: &mut Pcg64) -> Map {
    let blank = Tile { terrain: Terrain::Road, furn: Furniture::None, fuel: 0, fire: 0, building: NO_BUILDING };
    let mut map = Map { w: MAP_W, h: MAP_H, tiles: vec![blank; (MAP_W * MAP_H) as usize], buildings: vec![], payphones: vec![], original: vec![] };

    let mut kinds = vec![
        BuildingKind::Tenement,
        BuildingKind::Tenement,
        BuildingKind::Apartments,
        BuildingKind::Condos,
        BuildingKind::Diner,
        BuildingKind::Office,
        BuildingKind::Clinic,
        BuildingKind::CivicStation,
        BuildingKind::Bar,
        BuildingKind::Store,
        BuildingKind::Warehouse,
        BuildingKind::Park,
    ];
    kinds.shuffle(rng);

    for ly in 0..LOTS_Y {
        for lx in 0..LOTS_X {
            let x0 = STREET + lx * (LOT_W + STREET);
            let y0 = STREET + ly * (LOT_H + STREET);
            map.rect(x0, y0, LOT_W, LOT_H, Terrain::Sidewalk);
            let kind = kinds[(ly * LOTS_X + lx) as usize];
            let id = map.buildings.len();
            let mut b = Building {
                id,
                kind,
                name: building_name(kind, rng),
                x: x0 + 1,
                y: y0 + 1,
                w: LOT_W - 2,
                h: LOT_H - 2,
                entrances: vec![],
                units: vec![],
                work_spots: vec![],
                seats: vec![],
                service: vec![],
                beds: vec![],
                cells: vec![],
                register: None,
                till: 150,
                burned: 0,
            };
            build(&mut map, &mut b, rng);
            if kind != BuildingKind::Park {
                map.windows(&b);
            }
            for yy in b.y..b.y + b.h {
                for xx in b.x..b.x + b.w {
                    map.get_mut(Pos::new(xx, yy)).building = id as u8;
                }
            }
            map.buildings.push(b);
        }
    }

    // Payphones on a few sidewalk corners.
    for _ in 0..4 {
        for _ in 0..50 {
            let p = Pos::new(rng.gen_range(0..MAP_W), rng.gen_range(0..MAP_H));
            if map.get(p).terrain == Terrain::Sidewalk && map.get(p).building == NO_BUILDING {
                map.furn(p, Furniture::Payphone);
                map.payphones.push(p);
                break;
            }
        }
    }

    for t in map.tiles.iter_mut() {
        t.fuel = t.terrain.fuel().saturating_add(t.furn.fuel());
    }
    map.original = map.tiles.clone();
    map
}

fn build(map: &mut Map, b: &mut Building, rng: &mut Pcg64) {
    match b.kind {
        BuildingKind::Tenement => build_home(map, b, 4, 2, Terrain::Wood),
        BuildingKind::Apartments => build_home(map, b, 7, 2, Terrain::Wood),
        BuildingKind::Condos => build_home(map, b, 11, 1, Terrain::Carpet),
        BuildingKind::Diner => build_diner(map, b),
        BuildingKind::Office => build_office(map, b),
        BuildingKind::Clinic => build_clinic(map, b),
        BuildingKind::CivicStation => build_civic(map, b),
        BuildingKind::Bar => build_bar(map, b),
        BuildingKind::Store => build_store(map, b),
        BuildingKind::Warehouse => build_warehouse(map, b),
        BuildingKind::Park => build_park(map, b, rng),
    }
}

fn bottom_door(map: &mut Map, b: &mut Building) -> Pos {
    let d = Pos::new(b.x + b.w / 2, b.y + b.h - 1);
    map.set(d, Terrain::Door);
    b.entrances.push(d);
    d
}

fn build_home(map: &mut Map, b: &mut Building, room_w: i32, beds: usize, floor: Terrain) {
    let (x0, y0, w, h) = (b.x, b.y, b.w, b.h);
    map.room(x0, y0, w, h, floor);
    let cy = y0 + h / 2;
    for x in x0..x0 + w {
        map.set(Pos::new(x, cy - 1), Terrain::Wall);
        map.set(Pos::new(x, cy + 1), Terrain::Wall);
    }
    for d in [Pos::new(x0, cy), Pos::new(x0 + w - 1, cy)] {
        map.set(d, Terrain::Door);
        b.entrances.push(d);
    }
    let mut x = x0;
    while x + room_w + 1 <= x0 + w - 1 {
        let px = x + room_w + 1;
        for y in y0 + 1..cy - 1 {
            map.set(Pos::new(px, y), Terrain::Wall);
        }
        for y in cy + 2..y0 + h - 1 {
            map.set(Pos::new(px, y), Terrain::Wall);
        }
        let dx = x + 1 + room_w / 2;
        // upper room
        map.set(Pos::new(dx, cy - 1), Terrain::Door);
        let mut unit = vec![Pos::new(x + 1, y0 + 1)];
        if beds > 1 {
            unit.push(Pos::new(x + room_w, y0 + 1));
        }
        for p in &unit {
            map.furn(*p, Furniture::Bed);
        }
        if room_w >= 7 {
            map.furn(Pos::new(x + room_w / 2 + 1, y0 + 1), Furniture::Table);
        }
        if room_w >= 11 {
            map.furn(Pos::new(x + 3, y0 + 1), Furniture::Sofa);
        }
        b.units.push(unit);
        // lower room
        map.set(Pos::new(dx, cy + 1), Terrain::Door);
        let mut unit = vec![Pos::new(x + 1, y0 + h - 2)];
        if beds > 1 {
            unit.push(Pos::new(x + room_w, y0 + h - 2));
        }
        for p in &unit {
            map.furn(*p, Furniture::Bed);
        }
        if room_w >= 7 {
            map.furn(Pos::new(x + room_w / 2 + 1, y0 + h - 2), Furniture::Table);
        }
        if room_w >= 11 {
            map.furn(Pos::new(x + 3, y0 + h - 2), Furniture::Sofa);
        }
        b.units.push(unit);
        x += room_w + 1;
    }
    // corridor seats for loitering neighbours
    b.seats.push(Pos::new(x0 + 2, cy));
    b.seats.push(Pos::new(x0 + w - 3, cy));
}

fn build_diner(map: &mut Map, b: &mut Building) {
    let (x0, y0, w, h) = (b.x, b.y, b.w, b.h);
    map.room(x0, y0, w, h, Terrain::Tile);
    let door = bottom_door(map, b);
    // kitchen: stoves on the top row, counter on row y0+4 with gaps at both ends
    for x in (x0 + 3..x0 + w - 3).step_by(4) {
        map.furn(Pos::new(x, y0 + 1), Furniture::Stove);
        b.work_spots.push((Pos::new(x, y0 + 2), SpotKind::Kitchen));
    }
    for x in x0 + 2..x0 + w - 2 {
        map.furn(Pos::new(x, y0 + 4), Furniture::Counter);
    }
    let reg = Pos::new(x0 + w - 4, y0 + 4);
    map.furn(reg, Furniture::Register);
    b.register = Some(reg);
    b.work_spots.push((Pos::new(x0 + w - 4, y0 + 3), SpotKind::Counter));
    b.work_spots.push((Pos::new(x0 + w - 7, y0 + 3), SpotKind::Counter));
    b.service.push(Pos::new(x0 + w - 4, y0 + 5));
    for x in (x0 + 3..x0 + w - 5).step_by(2) {
        b.seats.push(Pos::new(x, y0 + 5));
        map.furn(Pos::new(x, y0 + 5), Furniture::Chair);
    }
    for ty in (y0 + 7..y0 + h - 2).step_by(3) {
        for tx in (x0 + 3..x0 + w - 3).step_by(5) {
            if (tx - door.x).abs() <= 2 {
                continue;
            }
            map.furn(Pos::new(tx, ty), Furniture::Table);
            for cx in [tx - 1, tx + 1] {
                map.furn(Pos::new(cx, ty), Furniture::Chair);
                b.seats.push(Pos::new(cx, ty));
            }
        }
    }
    map.furn(Pos::new(x0 + 1, y0 + h - 2), Furniture::Trash);
}

fn build_office(map: &mut Map, b: &mut Building) {
    let (x0, y0, w, h) = (b.x, b.y, b.w, b.h);
    map.room(x0, y0, w, h, Terrain::Carpet);
    let door = bottom_door(map, b);
    b.work_spots.push((door.add(-1, -1), SpotKind::Door));
    // corner office (top right)
    let cx0 = x0 + w - 9;
    for y in y0..y0 + 6 {
        map.set(Pos::new(cx0, y), Terrain::Wall);
    }
    for x in cx0..x0 + w {
        map.set(Pos::new(x, y0 + 5), Terrain::Wall);
    }
    map.set(Pos::new(cx0 + 4, y0 + 5), Terrain::Door);
    map.furn(Pos::new(cx0 + 3, y0 + 2), Furniture::Desk);
    map.furn(Pos::new(cx0 + 6, y0 + 2), Furniture::Desk);
    map.furn(Pos::new(x0 + w - 2, y0 + 4), Furniture::Sofa);
    b.work_spots.push((Pos::new(cx0 + 3, y0 + 1), SpotKind::Boss));
    b.work_spots.push((Pos::new(cx0 + 6, y0 + 1), SpotKind::Boss));
    // cubicle desks
    for y in (y0 + 2..y0 + h - 4).step_by(3) {
        for x in (x0 + 2..cx0 - 1).step_by(4) {
            map.furn(Pos::new(x, y), Furniture::Desk);
            map.furn(Pos::new(x, y + 1), Furniture::Chair);
            b.work_spots.push((Pos::new(x, y + 1), SpotKind::Desk));
        }
    }
    for x in (cx0 + 1..x0 + w - 2).step_by(3) {
        b.work_spots.push((Pos::new(x, y0 + 8), SpotKind::Floor));
    }
    map.furn(Pos::new(x0 + 1, y0 + h - 2), Furniture::Toilet);
    map.furn(Pos::new(x0 + w - 2, y0 + h - 2), Furniture::Trash);
    b.seats.push(Pos::new(x0 + w - 2, y0 + 4));
}

fn build_clinic(map: &mut Map, b: &mut Building) {
    let (x0, y0, w, h) = (b.x, b.y, b.w, b.h);
    map.room(x0, y0, w, h, Terrain::Tile);
    let door = bottom_door(map, b);
    for x in (x0 + 2..x0 + w - 2).step_by(3) {
        map.furn(Pos::new(x, y0 + 1), Furniture::Bed);
        b.beds.push(Pos::new(x, y0 + 1));
        b.work_spots.push((Pos::new(x, y0 + 3), SpotKind::Floor));
    }
    map.furn(Pos::new(door.x + 3, y0 + h - 4), Furniture::Counter);
    b.work_spots.push((Pos::new(door.x + 3, y0 + h - 5), SpotKind::Counter));
    map.furn(Pos::new(x0 + 3, y0 + 7), Furniture::Desk);
    b.work_spots.push((Pos::new(x0 + 3, y0 + 8), SpotKind::Desk));
    for x in (x0 + 2..door.x - 2).step_by(2) {
        map.furn(Pos::new(x, y0 + h - 2), Furniture::Chair);
        b.seats.push(Pos::new(x, y0 + h - 2));
    }
}

fn build_civic(map: &mut Map, b: &mut Building) {
    let (x0, y0, w, h) = (b.x, b.y, b.w, b.h);
    map.room(x0, y0, w, h, Terrain::Concrete);
    bottom_door(map, b);
    // three cells along the top-left
    for i in 0..3 {
        let cx = x0 + i * 4;
        map.room(cx, y0, 5, 5, Terrain::Concrete);
        map.set(Pos::new(cx + 2, y0 + 4), Terrain::Door);
        map.furn(Pos::new(cx + 1, y0 + 1), Furniture::Bench);
        b.cells.push(Pos::new(cx + 2, y0 + 2));
    }
    for x in (x0 + 2..x0 + 12).step_by(3) {
        map.furn(Pos::new(x, y0 + 8), Furniture::Desk);
        b.work_spots.push((Pos::new(x, y0 + 9), SpotKind::Desk));
    }
    // firehouse bay on the right
    for y in (y0 + 2..y0 + h - 2).step_by(3) {
        b.work_spots.push((Pos::new(x0 + w - 4, y), SpotKind::Floor));
    }
    map.furn(Pos::new(x0 + w - 2, y0 + 1), Furniture::Sofa);
    b.seats.push(Pos::new(x0 + 15, y0 + h - 2));
}

fn build_bar(map: &mut Map, b: &mut Building) {
    let (x0, y0, w, h) = (b.x, b.y, b.w, b.h);
    map.room(x0, y0, w, h, Terrain::Wood);
    let door = bottom_door(map, b);
    b.work_spots.push((door.add(1, -1), SpotKind::Door));
    for x in x0 + 3..x0 + w - 3 {
        map.furn(Pos::new(x, y0 + 3), Furniture::BarCounter);
    }
    let reg = Pos::new(x0 + 4, y0 + 3);
    map.furn(reg, Furniture::Register);
    b.register = Some(reg);
    for x in [x0 + 6, x0 + 14] {
        b.work_spots.push((Pos::new(x, y0 + 2), SpotKind::Counter));
    }
    for x in (x0 + 5..x0 + w - 4).step_by(2) {
        map.furn(Pos::new(x, y0 + 4), Furniture::Chair);
        b.seats.push(Pos::new(x, y0 + 4));
        b.service.push(Pos::new(x, y0 + 4));
    }
    for ty in [y0 + 8, y0 + 11] {
        for tx in (x0 + 3..x0 + w - 3).step_by(6) {
            if (tx - door.x).abs() <= 2 {
                continue;
            }
            map.furn(Pos::new(tx, ty), Furniture::Table);
            for cx in [tx - 1, tx + 1] {
                map.furn(Pos::new(cx, ty), Furniture::Chair);
                b.seats.push(Pos::new(cx, ty));
            }
        }
    }
    map.furn(Pos::new(x0 + 1, y0 + h - 2), Furniture::Sofa);
    b.seats.push(Pos::new(x0 + 1, y0 + h - 2));
    map.furn(Pos::new(x0 + w - 2, y0 + 1), Furniture::Toilet);
}

fn build_store(map: &mut Map, b: &mut Building) {
    let (x0, y0, w, h) = (b.x, b.y, b.w, b.h);
    map.room(x0, y0, w, h, Terrain::Tile);
    let door = bottom_door(map, b);
    for x in (x0 + 3..x0 + w - 3).step_by(3) {
        if (x - door.x).abs() <= 1 {
            continue;
        }
        for y in y0 + 2..y0 + h - 6 {
            map.furn(Pos::new(x, y), Furniture::Shelf);
        }
    }
    let reg = Pos::new(door.x + 3, y0 + h - 3);
    map.furn(reg, Furniture::Register);
    map.furn(reg.add(1, 0), Furniture::Counter);
    b.register = Some(reg);
    b.work_spots.push((reg.add(0, 1), SpotKind::Counter));
    b.service.push(reg.add(-1, 0));
    b.service.push(reg.add(0, -1));
    b.work_spots.push((Pos::new(x0 + 4, y0 + 1), SpotKind::Floor));
    map.furn(Pos::new(x0 + 1, y0 + h - 2), Furniture::Trash);
}

fn build_warehouse(map: &mut Map, b: &mut Building) {
    let (x0, y0, w, h) = (b.x, b.y, b.w, b.h);
    map.room(x0, y0, w, h, Terrain::Concrete);
    bottom_door(map, b);
    let d2 = Pos::new(x0, y0 + h / 2);
    map.set(d2, Terrain::Door);
    b.entrances.push(d2);
    for y in (y0 + 2..y0 + h - 4).step_by(3) {
        for x in (x0 + 3..x0 + w - 6).step_by(5) {
            map.furn(Pos::new(x, y), Furniture::Crate);
            map.furn(Pos::new(x + 1, y), Furniture::Crate);
            b.work_spots.push((Pos::new(x, y + 1), SpotKind::Floor));
        }
    }
    map.furn(Pos::new(x0 + w - 3, y0 + 2), Furniture::Desk);
    b.work_spots.push((Pos::new(x0 + w - 3, y0 + 3), SpotKind::Desk));
}

fn build_park(map: &mut Map, b: &mut Building, rng: &mut Pcg64) {
    let (x0, y0, w, h) = (b.x, b.y, b.w, b.h);
    map.rect(x0, y0, w, h, Terrain::Grass);
    let cy = y0 + h / 2;
    let cx = x0 + w / 2;
    for x in x0..x0 + w {
        map.set(Pos::new(x, cy), Terrain::Sidewalk);
    }
    for y in y0..y0 + h {
        map.set(Pos::new(cx, y), Terrain::Sidewalk);
    }
    for x in (x0 + 2..x0 + w - 2).step_by(4) {
        for y in [cy - 1, cy + 1] {
            if x != cx {
                map.furn(Pos::new(x, y), Furniture::Bench);
                b.seats.push(Pos::new(x, y));
            }
        }
    }
    for _ in 0..14 {
        let p = Pos::new(rng.gen_range(x0..x0 + w), rng.gen_range(y0..y0 + h));
        if map.get(p).terrain == Terrain::Grass && map.get(p).furn == Furniture::None {
            map.furn(p, Furniture::Tree);
        }
    }
    map.furn(Pos::new(cx + 1, cy + 2), Furniture::Trash);
    b.entrances.push(Pos::new(cx, y0));
}
