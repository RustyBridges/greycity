//! Grey City: the window, the ASCII renderer and the keyboard.
//! All game logic lives in the `greycity-sim` crate.

use greycity_sim::map::{Furniture, Pos, Terrain, LOT_H, LOT_W, STREET};
use greycity_sim::person::{opinion_word, ItemKind, MemKind, PART_NAMES};
use greycity_sim::player::{Bump, Interact, ShopKind};
use greycity_sim::world::{Busy, LogKind, ObjKind, Role, StartJob, World};
use macroquad::prelude::*;
use std::collections::HashMap;

fn window_conf() -> Conf {
    Conf { window_title: "Grey City".to_owned(), window_width: 1500, window_height: 900, high_dpi: true, ..Default::default() }
}

const BG: Color = Color::new(0.045, 0.045, 0.06, 1.0);
fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}
fn dim(c: Color, f: f32) -> Color {
    Color::new(c.r * f, c.g * f, c.b * f, 1.0)
}

#[derive(Clone)]
enum Act {
    Interact(usize, Interact),
    Buy(Option<ItemKind>),
    StealTill(Pos),
    Accept,
    Decline,
    Use(usize),
    Close,
}

#[derive(Clone)]
struct Menu {
    title: String,
    body: Vec<String>,
    options: Vec<(String, Act)>,
}

#[derive(Clone, Copy, PartialEq)]
enum DirAct {
    Ignite,
    Attack,
    Talk,
}

#[derive(Clone)]
enum Mode {
    Title,
    Play,
    Menu(Menu),
    Character,
    Help,
    Look(Pos),
    Dir(DirAct),
    Dead,
}

struct App {
    world: Option<World>,
    mode: Mode,
    seed: u64,
    font: f32,
    cw: f32,
    ch: f32,
    held: Option<(i32, i32)>,
    held_t: f32,
    log_scroll: usize,
    hover: Option<Pos>,
}

fn title_jobs(w: &World) -> Vec<usize> {
    (0..w.jobs.len()).filter(|j| w.jobs[*j].tier <= 2).collect()
}

impl App {
    fn new() -> App {
        let seed = (macroquad::miniquad::date::now() * 1000.0) as u64 % 1_000_000;
        let mut a = App { world: None, mode: Mode::Title, seed, font: 20.0, cw: 10.0, ch: 20.0, held: None, held_t: 0.0, log_scroll: 0, hover: None };
        a.measure();
        // A throwaway world, so the title screen can list jobs.
        a.world = Some(World::new(seed, StartJob::Random));
        a
    }

    fn measure(&mut self) {
        let d = measure_text("M", None, self.font as u16, 1.0);
        self.cw = d.width.ceil().max(6.0);
        self.ch = (self.font * 1.0).ceil();
    }

    fn w(&mut self) -> &mut World {
        self.world.as_mut().unwrap()
    }

    // ------------------------------------------------------------ input

    fn dir_from_keys(&mut self) -> Option<(i32, i32)> {
        let map = [
            (KeyCode::Up, (0, -1)),
            (KeyCode::Down, (0, 1)),
            (KeyCode::Left, (-1, 0)),
            (KeyCode::Right, (1, 0)),
            (KeyCode::Kp8, (0, -1)),
            (KeyCode::Kp2, (0, 1)),
            (KeyCode::Kp4, (-1, 0)),
            (KeyCode::Kp6, (1, 0)),
            (KeyCode::Kp7, (-1, -1)),
            (KeyCode::Kp9, (1, -1)),
            (KeyCode::Kp1, (-1, 1)),
            (KeyCode::Kp3, (1, 1)),
        ];
        for (k, d) in map {
            if is_key_pressed(k) {
                self.held = Some(d);
                self.held_t = -0.22;
                return Some(d);
            }
        }
        if let Some(d) = self.held {
            let still = map.iter().any(|(k, dd)| *dd == d && is_key_down(*k));
            if !still {
                self.held = None;
            } else {
                self.held_t += get_frame_time();
                if self.held_t > 0.055 {
                    self.held_t = 0.0;
                    return Some(d);
                }
            }
        }
        None
    }

    fn char_dir(c: char) -> Option<(i32, i32)> {
        Some(match c {
            'h' => (-1, 0),
            'l' => (1, 0),
            'k' => (0, -1),
            'j' => (0, 1),
            'y' => (-1, -1),
            'u' => (1, -1),
            'b' => (-1, 1),
            'n' => (1, 1),
            _ => return None,
        })
    }

    fn update(&mut self) {
        let chars: Vec<char> = std::iter::from_fn(get_char_pressed).collect();
        if is_key_pressed(KeyCode::Equal) || chars.contains(&'+') {
            self.font = (self.font + 2.0).min(36.0);
            self.measure();
        }
        if is_key_pressed(KeyCode::Minus) {
            self.font = (self.font - 2.0).max(10.0);
            self.measure();
        }
        let chars: Vec<char> = chars.into_iter().filter(|c| *c != '+' && *c != '-' && *c != '=').collect();
        let (mx, my) = mouse_position();
        self.hover = self.screen_to_map(mx, my);

        match self.mode.clone() {
            Mode::Title => self.update_title(&chars),
            Mode::Play => self.update_play(&chars),
            Mode::Menu(m) => self.update_menu(m, &chars),
            Mode::Character | Mode::Help => {
                if is_key_pressed(KeyCode::Escape) || !chars.is_empty() || is_key_pressed(KeyCode::Enter) {
                    self.mode = Mode::Play;
                }
            }
            Mode::Look(p) => {
                let d = self.dir_from_keys().or_else(|| chars.iter().find_map(|c| Self::char_dir(*c)));
                if let Some((dx, dy)) = d {
                    let np = p.add(dx, dy);
                    if self.w().map.in_bounds(np) {
                        self.mode = Mode::Look(np);
                    }
                }
                if is_key_pressed(KeyCode::Escape) || chars.contains(&'x') || chars.contains(&'q') {
                    self.mode = Mode::Play;
                }
            }
            Mode::Dir(act) => {
                let d = self.dir_from_keys().or_else(|| chars.iter().find_map(|c| Self::char_dir(*c)));
                if is_key_pressed(KeyCode::Escape) {
                    self.mode = Mode::Play;
                } else if let Some((dx, dy)) = d {
                    self.mode = Mode::Play;
                    self.held = None;
                    let w = self.w();
                    match act {
                        DirAct::Ignite => w.act_ignite(dx, dy),
                        DirAct::Attack => match w.bump(dx, dy) {
                            Bump::Person(n) | Bump::Hostile(n) => w.act_interact(n, Interact::Attack),
                            _ => w.say(None, &[w.player], LogKind::Normal, "You swing at the air. The air wins."),
                        },
                        DirAct::Talk => match w.bump(dx, dy) {
                            Bump::Person(n) | Bump::Hostile(n) => self.open_interact(n),
                            _ => {
                                let pid = w.player;
                                w.say(None, &[pid], LogKind::Normal, "You talk to nobody. Nobody listens, as usual.")
                            }
                        },
                    }
                }
            }
            Mode::Dead => {
                if chars.contains(&'r') || is_key_pressed(KeyCode::Enter) {
                    self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) % 1_000_000;
                    self.world = Some(World::new(self.seed, StartJob::Random));
                    self.mode = Mode::Title;
                }
            }
        }
    }

    fn update_title(&mut self, chars: &[char]) {
        let jobs = title_jobs(self.world.as_ref().unwrap());
        let start = |a: &mut App, s: StartJob| {
            a.world = Some(World::new(a.seed, s));
            a.mode = Mode::Play;
            a.log_scroll = 0;
        };
        if is_key_pressed(KeyCode::Enter) {
            start(self, StartJob::Random);
            return;
        }
        for c in chars {
            match c {
                'R' => return start(self, StartJob::Random),
                'U' | '0' => return start(self, StartJob::Unemployed),
                'S' => {
                    self.seed = (self.seed * 7919 + 13) % 1_000_000;
                    self.world = Some(World::new(self.seed, StartJob::Random));
                }
                c if c.is_ascii_lowercase() => {
                    let k = (*c as u8 - b'a') as usize;
                    if let Some(&j) = jobs.get(k) {
                        return start(self, StartJob::Job(j));
                    }
                }
                _ => {}
            }
        }
    }

    fn update_menu(&mut self, m: Menu, chars: &[char]) {
        if is_key_pressed(KeyCode::Escape) {
            // Escape on an offer declines it.
            if m.options.iter().any(|(_, a)| matches!(a, Act::Decline)) {
                self.w().decline_offer();
            }
            self.mode = Mode::Play;
            return;
        }
        for c in chars {
            if !c.is_ascii_lowercase() {
                continue;
            }
            let k = (*c as u8 - b'a') as usize;
            if let Some((_, act)) = m.options.get(k).cloned() {
                self.mode = Mode::Play;
                let w = self.w();
                match act {
                    Act::Interact(n, i) => w.act_interact(n, i),
                    Act::Buy(it) => w.act_buy(it),
                    Act::StealTill(p) => w.act_steal_till(p),
                    Act::Accept => w.accept_offer(),
                    Act::Decline => w.decline_offer(),
                    Act::Use(i) => w.act_use(i),
                    Act::Close => {}
                }
                return;
            }
        }
    }

    fn open_interact(&mut self, n: usize) {
        let w = self.world.as_ref().unwrap();
        let p = &w.people[n];
        let mut body = vec![format!("{}, {}. {}.", p.name, p.age, w.job_name(n))];
        body.push(format!("{} {} you. Currently {}.", p.first_name(), opinion_word(p.opinion(w.player)), w.describe_activity(n)));
        let cond = w.describe_condition(n);
        if !cond.is_empty() {
            body.push(format!("They look {}.", cond.join(", ")));
        }
        let options = Interact::ALL.iter().map(|i| (i.label().to_string(), Act::Interact(n, *i))).collect();
        self.mode = Mode::Menu(Menu { title: p.name.clone(), body, options });
    }

    fn open_counter(&mut self, at: Pos, furn: Furniture) {
        let w = self.world.as_ref().unwrap();
        let pp = w.player_pos();
        let shop = w.shop_here(at);
        let mut options = vec![];
        let mut body = vec![];
        match shop {
            ShopKind::Store => {
                body.push("Fluorescent light. A radio plays a song about somewhere else.".into());
                for it in ItemKind::SHOP {
                    options.push((format!("Buy {} (${})", it.name(), it.price()), Act::Buy(Some(it))));
                }
            }
            ShopKind::Diner => {
                body.push("Today's special is beige. Yesterday's special was also beige.".into());
                options.push(("Order the special ($9)".into(), Act::Buy(None)));
            }
            ShopKind::Bar => {
                body.push("The bartender is polishing a glass that will never be clean.".into());
                options.push(("Order a drink ($6)".into(), Act::Buy(None)));
            }
            ShopKind::None => {}
        }
        if furn == Furniture::Register {
            options.push(("Empty the till when nobody is looking (crime)".into(), Act::StealTill(at)));
        }
        if options.is_empty() {
            let pid = w.player;
            self.w().say(None, &[pid], LogKind::Normal, format!("A {}. Nothing to do with it.", furn.name()));
            return;
        }
        let _ = pp;
        options.push(("Never mind".into(), Act::Close));
        let title = w.map.building_at(at).map(|b| w.map.buildings[b].name.clone()).unwrap_or_default();
        self.mode = Mode::Menu(Menu { title, body, options });
    }

    fn update_play(&mut self, chars: &[char]) {
        let w = self.world.as_mut().unwrap();
        if !w.player_alive() {
            self.mode = Mode::Dead;
            return;
        }
        // Offers interrupt everything.
        if let Some((obj, speech)) = w.offer.clone() {
            let reward = format!("Reward: ${}. {}", obj.reward, obj.summary);
            self.mode = Mode::Menu(Menu {
                title: "An offer".into(),
                body: vec![speech, String::new(), reward],
                options: vec![("Accept".into(), Act::Accept), ("Decline".into(), Act::Decline)],
            });
            return;
        }
        // Time passes on its own while busy, asleep, unconscious or jailed.
        let pid = w.player;
        let now = w.turn;
        let incapacitated = !w.people[pid].can_act(now);
        if w.busy.is_some() || incapacitated {
            let per_frame = match w.busy {
                Some(Busy::Jail) => 240,
                Some(Busy::Sleep) | Some(Busy::Work) => 50,
                _ if incapacitated => 20,
                _ => 15,
            };
            for _ in 0..per_frame {
                w.step();
                if (w.busy.is_none() && w.people[pid].can_act(w.turn)) || !w.player_alive() || w.offer.is_some() {
                    break;
                }
            }
            if is_key_pressed(KeyCode::Escape) && w.busy != Some(Busy::Jail) {
                w.busy = None;
                w.people[pid].sleeping = false;
            }
            self.log_scroll = 0;
            return;
        }

        if is_key_pressed(KeyCode::PageUp) {
            self.log_scroll += 5;
        }
        if is_key_pressed(KeyCode::PageDown) {
            self.log_scroll = self.log_scroll.saturating_sub(5);
        }
        if is_key_pressed(KeyCode::Kp5) {
            w.act_wait(1);
        }

        let mut dir = self.dir_from_keys();
        for c in chars {
            if dir.is_none() {
                dir = Self::char_dir(*c);
                if dir.is_some() {
                    continue;
                }
            }
            let w = self.world.as_mut().unwrap();
            match c {
                '.' | 's' => w.act_wait(1),
                'w' => w.act_wait(15),
                'W' => w.act_wait(60),
                'z' => w.act_sleep(),
                'e' => w.act_work(),
                'f' => self.mode = Mode::Dir(DirAct::Ignite),
                'a' => self.mode = Mode::Dir(DirAct::Attack),
                't' => self.mode = Mode::Dir(DirAct::Talk),
                'x' => {
                    let p = w.player_pos();
                    self.mode = Mode::Look(p);
                }
                'c' => self.mode = Mode::Character,
                '?' => self.mode = Mode::Help,
                'i' => {
                    let inv = w.people[w.player].inventory.clone();
                    let options: Vec<(String, Act)> = inv.iter().enumerate().map(|(k, it)| (format!("Use {}", it.name()), Act::Use(k))).collect();
                    let body = if options.is_empty() { vec!["Your pockets are empty, like a metaphor.".to_string()] } else { vec![] };
                    let mut options = options;
                    options.push(("Close".into(), Act::Close));
                    self.mode = Mode::Menu(Menu { title: "Pockets".into(), body, options });
                }
                _ => {}
            }
            if !matches!(self.mode, Mode::Play) {
                return;
            }
        }
        if let Some((dx, dy)) = dir {
            self.log_scroll = 0;
            let w = self.world.as_mut().unwrap();
            match w.bump(dx, dy) {
                Bump::Move => w.act_move(dx, dy),
                Bump::Hostile(n) => w.act_interact(n, Interact::Attack),
                Bump::Person(n) => {
                    self.held = None;
                    self.open_interact(n);
                }
                Bump::Furniture(Furniture::Payphone, _) => w.act_answer_phone(),
                Bump::Furniture(f @ (Furniture::Register | Furniture::Counter | Furniture::BarCounter), at) => {
                    self.held = None;
                    self.open_counter(at, f);
                }
                Bump::Furniture(f, _) => {
                    let pid = w.player;
                    w.say(None, &[pid], LogKind::Normal, format!("There's a {} in the way.", f.name()));
                    self.held = None;
                }
                Bump::Blocked => {}
            }
        }
    }

    // ------------------------------------------------------------ layout

    fn layout(&self) -> (i32, i32, i32, i32) {
        let cols = (screen_width() / self.cw) as i32;
        let rows = (screen_height() / self.ch) as i32;
        let side = 40.min(cols / 3);
        let log = 10.min(rows / 3);
        (cols - side, rows - log, side, log)
    }

    fn camera(&self) -> Pos {
        let w = self.world.as_ref().unwrap();
        let (vw, vh, _, _) = self.layout();
        let pp = match self.mode {
            Mode::Look(p) => p,
            _ => w.player_pos(),
        };
        let cx = if vw >= w.map.w { -(vw - w.map.w) / 2 } else { (pp.x - vw / 2).clamp(0, w.map.w - vw) };
        let cy = if vh >= w.map.h { -(vh - w.map.h) / 2 } else { (pp.y - vh / 2).clamp(0, w.map.h - vh) };
        Pos::new(cx, cy)
    }

    fn screen_to_map(&self, x: f32, y: f32) -> Option<Pos> {
        self.world.as_ref()?;
        let (vw, vh, _, _) = self.layout();
        let (cx, cy) = ((x / self.cw) as i32, (y / self.ch) as i32);
        if cx >= vw || cy >= vh {
            return None;
        }
        let cam = self.camera();
        let p = Pos::new(cam.x + cx, cam.y + cy);
        self.world.as_ref().unwrap().map.in_bounds(p).then_some(p)
    }

    fn put(&self, cx: i32, cy: i32, ch: char, fg: Color, bg: Option<Color>) {
        let x = cx as f32 * self.cw;
        let y = cy as f32 * self.ch;
        if let Some(b) = bg {
            draw_rectangle(x, y, self.cw, self.ch, b);
        }
        if ch != ' ' {
            let mut buf = [0u8; 4];
            draw_text(ch.encode_utf8(&mut buf), x, y + self.ch * 0.78, self.font, fg);
        }
    }

    fn text(&self, cx: i32, cy: i32, s: &str, c: Color) {
        draw_text(s, cx as f32 * self.cw, cy as f32 * self.ch + self.ch * 0.78, self.font, c);
    }

    // ------------------------------------------------------------ drawing

    fn draw(&self) {
        clear_background(BG);
        match &self.mode {
            Mode::Title => return self.draw_title(),
            Mode::Dead => return self.draw_dead(),
            _ => {}
        }
        self.draw_map();
        self.draw_side();
        self.draw_log();
        match &self.mode {
            Mode::Menu(m) => self.draw_menu(m),
            Mode::Character => self.draw_character(),
            Mode::Help => self.draw_help(),
            Mode::Dir(a) => {
                let (vw, vh, _, _) = self.layout();
                let s = match a {
                    DirAct::Ignite => "Set fire to what? (direction, Esc to cancel)",
                    DirAct::Attack => "Attack in which direction?",
                    DirAct::Talk => "Talk to whom? (direction)",
                };
                draw_rectangle(0.0, (vh - 1) as f32 * self.ch, vw as f32 * self.cw, self.ch, rgb(60, 20, 20));
                self.text(1, vh - 1, s, WHITE);
            }
            _ => {}
        }
    }

    fn tile_glyph(w: &World, p: Pos) -> (char, Color) {
        let t = w.map.get(p);
        if t.furn != Furniture::None {
            let (c, col) = match t.furn {
                Furniture::Bed => ('=', rgb(95, 115, 175)),
                Furniture::Table => ('o', rgb(150, 110, 70)),
                Furniture::Chair => ('h', rgb(140, 100, 65)),
                Furniture::Counter => ('[', rgb(150, 150, 150)),
                Furniture::Desk => ('n', rgb(150, 115, 80)),
                Furniture::Stove => ('&', rgb(220, 120, 50)),
                Furniture::Shelf => ('|', rgb(175, 145, 95)),
                Furniture::Bench => ('_', rgb(140, 105, 70)),
                Furniture::Tree => ('T', rgb(70, 120, 60)),
                Furniture::BarCounter => ('[', rgb(130, 80, 45)),
                Furniture::Register => ('$', rgb(90, 200, 110)),
                Furniture::Toilet => ('U', rgb(200, 200, 210)),
                Furniture::Crate => ('X', rgb(150, 115, 70)),
                Furniture::Washer => ('O', rgb(200, 200, 210)),
                Furniture::Payphone => ('P', rgb(80, 190, 200)),
                Furniture::Trash => ('x', rgb(110, 110, 100)),
                Furniture::Sofa => ('m', rgb(150, 80, 110)),
                Furniture::None => (' ', WHITE),
            };
            return (c, col);
        }
        let street_line_h = p.y % (LOT_H + STREET) == 1 && p.x % (LOT_W + STREET) >= STREET;
        let street_line_v = p.x % (LOT_W + STREET) == 1 && p.y % (LOT_H + STREET) >= STREET;
        match t.terrain {
            Terrain::Road if street_line_h && p.x % 4 < 2 => ('-', rgb(120, 115, 60)),
            Terrain::Road if street_line_v && p.y % 3 < 2 => ('|', rgb(120, 115, 60)),
            Terrain::Road => ('.', rgb(55, 55, 62)),
            Terrain::Sidewalk => ('.', rgb(110, 110, 115)),
            Terrain::Grass => ('"', rgb(85, 105, 60)),
            Terrain::Wood => ('.', rgb(125, 90, 55)),
            Terrain::Carpet => ('.', rgb(120, 55, 65)),
            Terrain::Tile => ('.', rgb(160, 160, 150)),
            Terrain::Concrete => ('.', rgb(100, 100, 100)),
            Terrain::Wall => ('#', rgb(145, 145, 155)),
            Terrain::Door => ('+', rgb(170, 115, 60)),
            Terrain::Window => (':', rgb(110, 165, 200)),
            Terrain::Ash => (',', rgb(80, 72, 66)),
        }
    }

    fn person_color(w: &World, i: usize) -> Color {
        let p = &w.people[i];
        let now = w.turn;
        if p.is_player {
            return rgb(255, 225, 70);
        }
        if p.hostile_to == Some(w.player) {
            return rgb(255, 80, 80);
        }
        if p.hum {
            return rgb(210, 120, 230);
        }
        let c = match w.role(i) {
            Role::Cop => rgb(90, 140, 255),
            Role::Firefighter => rgb(235, 85, 55),
            Role::Medic => rgb(110, 225, 150),
            _ => match w.tier(i) {
                0..=1 if p.job.is_some() => rgb(185, 170, 150),
                0..=1 => rgb(150, 140, 130),
                2..=3 => rgb(215, 215, 215),
                _ => rgb(205, 175, 255),
            },
        };
        if p.unconscious_until > now || p.sleeping || p.jailed_until > now {
            dim(c, 0.55)
        } else {
            c
        }
    }

    fn draw_map(&self) {
        let w = self.world.as_ref().unwrap();
        let (vw, vh, _, _) = self.layout();
        let cam = self.camera();
        let now = w.turn;
        let night = w.is_night();
        let t = get_time();
        let mut people: HashMap<Pos, usize> = HashMap::new();
        for p in &w.people {
            if p.cause_of_death.as_deref() == Some("left town") {
                continue;
            }
            let e = people.entry(p.pos).or_insert(p.id);
            if !w.people[*e].alive && p.alive || p.is_player {
                *e = p.id;
            }
        }
        let pp = w.player_pos();
        let work_spot = w.people[w.player].work_spot;
        let ringing = w.phone_ringing.map(|(p, _)| p);
        for cy in 0..vh {
            for cx in 0..vw {
                let p = Pos::new(cam.x + cx, cam.y + cy);
                if !w.map.in_bounds(p) {
                    continue;
                }
                let idx = w.map.idx(p);
                if !w.seen[idx] {
                    continue;
                }
                let vis = w.visible[idx];
                let tile = w.map.get(p);
                let (mut ch, mut fg) = Self::tile_glyph(w, p);
                let mut bg = None;
                if !vis {
                    self.put(cx, cy, ch, dim(fg, 0.32), None);
                    continue;
                }
                if night && !w.map.indoors(p) {
                    let d = p.dist(pp) as f32;
                    fg = dim(fg, (1.0 - d / 14.0).clamp(0.35, 0.85));
                }
                if Some(p) == work_spot && w.people[w.player].job.is_some() {
                    bg = Some(rgb(45, 45, 20));
                }
                if Some(p) == ringing && (t * 4.0) as i64 % 2 == 0 {
                    fg = rgb(255, 255, 255);
                    bg = Some(rgb(30, 90, 100));
                }
                if tile.fire > 0 {
                    let k = ((t * 9.0) as i64 + p.x as i64 * 7 + p.y as i64 * 13) % 3;
                    ch = '^';
                    fg = [rgb(255, 90, 20), rgb(255, 180, 30), rgb(255, 240, 140)][k as usize];
                    bg = Some(rgb(80 + tile.fire * 8, 20, 0));
                }
                if let Some(&i) = people.get(&p) {
                    let q = &w.people[i];
                    if !q.alive {
                        ch = '%';
                        fg = rgb(170, 40, 40);
                    } else {
                        ch = '@';
                        fg = Self::person_color(w, i);
                        if q.unconscious_until > now {
                            ch = '&';
                        }
                    }
                }
                self.put(cx, cy, ch, fg, bg);
            }
        }
        // Look cursor / hover highlight.
        let cursor = match self.mode {
            Mode::Look(p) => Some(p),
            _ => self.hover,
        };
        if let Some(c) = cursor {
            let (cx, cy) = (c.x - cam.x, c.y - cam.y);
            if cx >= 0 && cy >= 0 && cx < vw && cy < vh {
                draw_rectangle_lines(cx as f32 * self.cw, cy as f32 * self.ch, self.cw, self.ch, 1.5, rgb(230, 230, 120));
            }
        }
    }

    fn bar(&self, x: i32, y: i32, label: &str, v: f32, good_high: bool, word: &str) {
        let v = v.clamp(0.0, 100.0);
        let n = (v / 10.0).round() as usize;
        let bad = if good_high { 100.0 - v } else { v };
        let col = if bad > 70.0 {
            rgb(230, 80, 70)
        } else if bad > 45.0 {
            rgb(220, 180, 70)
        } else {
            rgb(130, 190, 120)
        };
        self.text(x, y, label, rgb(160, 160, 170));
        self.text(x + 8, y, &"#".repeat(n), col);
        self.text(x + 8 + n as i32, y, &".".repeat(10 - n), rgb(60, 60, 70));
        self.text(x + 20, y, word, col);
    }

    fn draw_side(&self) {
        let w = self.world.as_ref().unwrap();
        let (vw, _, side, _) = self.layout();
        let x = vw + 1;
        draw_rectangle(vw as f32 * self.cw, 0.0, side as f32 * self.cw, screen_height(), rgb(16, 16, 22));
        let pid = w.player;
        let p = &w.people[pid];
        let mut y = 0;
        let line = |s: &str, c: Color, y: &mut i32| {
            self.text(x, *y, s, c);
            *y += 1;
        };
        line(&format!("{}, {}", p.name, p.age), rgb(255, 225, 70), &mut y);
        let job = w.job_name(pid).to_string();
        let wp = p.workplace.map(|b| w.map.buildings[b].name.clone());
        line(&job, WHITE, &mut y);
        if let Some(wp) = wp {
            line(&format!("  at {}", wp), rgb(150, 150, 160), &mut y);
        }
        line(&format!("{} {}  (day {})", w.weekday(), w.clock(), w.day() + 1), rgb(180, 180, 200), &mut y);
        let money_c = if p.money < 0 { rgb(230, 80, 70) } else { rgb(110, 210, 120) };
        line(&format!("${}", p.money), money_c, &mut y);
        if p.home.is_some() {
            let days = (7 - w.day() % 7) % 7;
            let days = if days == 0 { 7 } else { days };
            let owed = if p.rent_owed > 0 { format!(" (+${} owed)", p.rent_owed) } else { String::new() };
            line(&format!("Rent ${} in {}d{}", w.rent_share(pid), days, owed), rgb(170, 170, 180), &mut y);
        } else {
            line("Homeless", rgb(230, 80, 70), &mut y);
        }
        y += 1;
        let word = |v: f32, ws: [&'static str; 4]| -> &'static str {
            match v as i32 {
                0..=24 => ws[0],
                25..=49 => ws[1],
                50..=74 => ws[2],
                _ => ws[3],
            }
        };
        self.bar(x, y, "Hunger", p.hunger, false, word(p.hunger, ["fed", "peckish", "hungry", "starving"]));
        y += 1;
        self.bar(x, y, "Energy", p.energy, true, word(p.energy, ["exhausted", "tired", "ok", "rested"]));
        y += 1;
        self.bar(x, y, "Stress", p.stress, false, word(p.stress, ["calm", "uneasy", "tense", "breaking"]));
        y += 1;
        self.bar(x, y, "Lonely", p.lonely, false, word(p.lonely, ["", "", "lonely", "isolated"]));
        y += 1;
        if p.drunk > 1.0 {
            self.bar(x, y, "Drunk", p.drunk, false, word(p.drunk, ["buzzed", "tipsy", "drunk", "wasted"]));
            y += 1;
        }
        // Body
        let mut bx = x;
        for k in 0..6 {
            let frac = p.body.hp[k] as f32 / p.body.max[k] as f32;
            let c = if frac <= 0.0 {
                rgb(90, 30, 30)
            } else if frac < 0.4 {
                rgb(230, 70, 60)
            } else if frac < 0.8 {
                rgb(220, 180, 70)
            } else {
                rgb(130, 190, 120)
            };
            let label = ["Hd", "To", "LA", "RA", "LL", "RL"][k];
            self.text(bx, y, label, c);
            bx += 3;
        }
        y += 1;
        if p.body.bleeding > 0 {
            line(&format!("BLEEDING (blood {:.0}%)", p.body.blood), rgb(240, 60, 60), &mut y);
        }
        y += 1;
        // Work
        if p.job.is_some() {
            if w.on_shift(pid) {
                if w.busy == Some(Busy::Work) {
                    line("Working...", rgb(130, 190, 120), &mut y);
                } else {
                    line("ON SHIFT: go to your station, press e", rgb(230, 180, 70), &mut y);
                }
            } else if let Some(m) = w.minutes_until_shift(pid) {
                line(&format!("Next shift in {}h{:02}m", m / 60, m % 60), rgb(160, 160, 170), &mut y);
            }
            if p.missed_shifts > 0 {
                line(&format!("Missed shifts: {}/3", p.missed_shifts), rgb(230, 80, 70), &mut y);
            }
        }
        let heat = w.heat(pid);
        if heat > 0 {
            line(&format!("WANTED (heat {})", heat), rgb(255, 80, 80), &mut y);
        }
        if p.jailed_until > w.turn {
            line(&format!("In jail for {}h", (p.jailed_until - w.turn) / 60 + 1), rgb(255, 80, 80), &mut y);
        }
        let open: Vec<_> = w.objectives.iter().filter(|o| !o.done && !o.failed).collect();
        if !open.is_empty() {
            y += 1;
            line("Secret business:", rgb(200, 120, 230), &mut y);
            for o in open {
                let left = o.deadline.saturating_sub(w.turn);
                let s = format!("- {} [{}d{}h]", o.summary, left / 1440, (left % 1440) / 60);
                for chunk in wrap(&s, (side - 2) as usize) {
                    line(&chunk, rgb(190, 150, 210), &mut y);
                }
            }
        }
        y += 1;
        // Look panel
        let target = match self.mode {
            Mode::Look(p) => Some(p),
            _ => self.hover,
        };
        if let Some(t) = target {
            if w.seen[w.map.idx(t)] {
                for s in self.describe(t) {
                    for chunk in wrap(&s, (side - 2) as usize) {
                        line(&chunk, rgb(200, 200, 170), &mut y);
                    }
                }
            }
        }
        let rows = (screen_height() / self.ch) as i32;
        self.text(x, rows - 2, "? help  c character  i pockets", rgb(100, 100, 115));
        self.text(x, rows - 1, "seed ", rgb(70, 70, 80));
        self.text(x + 5, rows - 1, &w.seed.to_string(), rgb(70, 70, 80));
    }

    fn describe(&self, t: Pos) -> Vec<String> {
        let w = self.world.as_ref().unwrap();
        let mut out = vec![];
        let vis = w.visible[w.map.idx(t)];
        if vis {
            for q in w.people.iter().filter(|q| q.pos == t && q.cause_of_death.as_deref() != Some("left town")) {
                if !q.alive {
                    out.push(format!("The body of {}. ({})", q.name, q.cause_of_death.clone().unwrap_or_default()));
                    continue;
                }
                if q.is_player {
                    out.push("You.".into());
                    continue;
                }
                out.push(format!("{}, {}, {}", q.name, q.age, w.job_name(q.id)));
                out.push(format!("  {} you. {}.", opinion_word(q.opinion(w.player)), w.describe_activity(q.id)));
                let cond = w.describe_condition(q.id);
                if !cond.is_empty() {
                    out.push(format!("  Looks {}.", cond.join(", ")));
                }
            }
        }
        let tile = w.map.get(t);
        let mut s = String::new();
        if tile.fire > 0 && vis {
            s.push_str("FIRE. ");
        }
        if tile.furn != Furniture::None {
            s.push_str(&format!("A {} on {}", tile.furn.name(), tile.terrain.name()));
        } else {
            s.push_str(&capital(tile.terrain.name()));
        }
        if let Some(b) = w.map.building_at(t) {
            s.push_str(&format!(", {}", w.map.buildings[b].name));
        } else if matches!(tile.terrain, Terrain::Road | Terrain::Sidewalk) {
            s.push_str(&format!(", {}", w.map.street_name(t)));
        }
        out.push(s + ".");
        out
    }

    fn draw_log(&self) {
        let w = self.world.as_ref().unwrap();
        let (vw, vh, _, lh) = self.layout();
        draw_rectangle(0.0, vh as f32 * self.ch, vw as f32 * self.cw, lh as f32 * self.ch, rgb(10, 10, 14));
        let width = (vw - 8).max(20) as usize;
        let mut lines: Vec<(String, Color, bool)> = vec![];
        for e in w.log.iter().rev().filter(|e| e.seen).take(60) {
            let c = log_color(e.kind);
            let ts = format!("{:02}:{:02}", (e.turn % 1440) / 60, e.turn % 60);
            let wrapped = wrap(&e.text, width);
            let n = wrapped.len();
            for (k, l) in wrapped.into_iter().enumerate().rev() {
                lines.push((if k == 0 { format!("{} {}", ts, l) } else { format!("      {}", l) }, c, w.turn - e.turn < 2));
                let _ = n;
            }
            if lines.len() > lh as usize + self.log_scroll + 2 {
                break;
            }
        }
        let start = self.log_scroll.min(lines.len().saturating_sub(1));
        for (k, (s, c, fresh)) in lines.iter().skip(start).take(lh as usize).enumerate() {
            let y = vh + lh - 1 - k as i32;
            let c = if *fresh { *c } else { dim(*c, 0.8) };
            self.text(1, y, s, c);
        }
        if self.log_scroll > 0 {
            self.text(vw - 12, vh, "[scrolled]", rgb(120, 120, 60));
        }
    }

    fn panel(&self, w_cols: i32, h_rows: i32) -> (i32, i32) {
        let cols = (screen_width() / self.cw) as i32;
        let rows = (screen_height() / self.ch) as i32;
        let x = ((cols - w_cols) / 2).max(0);
        let y = ((rows - h_rows) / 2).max(0);
        draw_rectangle(x as f32 * self.cw, y as f32 * self.ch, w_cols as f32 * self.cw, h_rows as f32 * self.ch, rgb(20, 20, 28));
        draw_rectangle_lines(x as f32 * self.cw, y as f32 * self.ch, w_cols as f32 * self.cw, h_rows as f32 * self.ch, 2.0, rgb(110, 110, 130));
        (x, y)
    }

    fn draw_menu(&self, m: &Menu) {
        let width: usize = 64;
        let body: Vec<String> = m.body.iter().flat_map(|b| if b.is_empty() { vec![String::new()] } else { wrap(b, width - 4) }).collect();
        let h = 4 + body.len() as i32 + m.options.len() as i32 + 1;
        let (x, y) = self.panel(width as i32, h);
        self.text(x + 2, y + 1, &m.title, rgb(255, 225, 70));
        let mut yy = y + 2;
        for b in &body {
            self.text(x + 2, yy, b, rgb(200, 200, 200));
            yy += 1;
        }
        yy += 1;
        for (k, (label, act)) in m.options.iter().enumerate() {
            let c = match act {
                Act::Interact(_, Interact::Attack) | Act::Interact(_, Interact::Pickpocket) | Act::StealTill(_) => rgb(230, 110, 100),
                Act::Accept => rgb(200, 120, 230),
                _ => rgb(170, 200, 230),
            };
            self.text(x + 2, yy, &format!("{}) {}", (b'a' + k as u8) as char, label), c);
            yy += 1;
        }
    }

    fn draw_character(&self) {
        let w = self.world.as_ref().unwrap();
        let pid = w.player;
        let p = &w.people[pid];
        let (x, y) = self.panel(76, 34);
        let mut yy = y + 1;
        let mut line = |s: String, c: Color| {
            self.text(x + 2, yy, &s, c);
            yy += 1;
        };
        line(format!("{}, {}: {}", p.name, p.age, w.job_name(pid)), rgb(255, 225, 70));
        line(format!("You are {}.", p.traits.iter().map(|t| t.name()).collect::<Vec<_>>().join(", ")), WHITE);
        let home = p.home.map(|h| w.map.buildings[h].name.clone()).unwrap_or("nowhere".into());
        line(format!("Home: {}.  Record: {} arrest(s).  Shifts worked: {}.", home, p.record, w.stats.player_shifts), rgb(180, 180, 190));
        if let Some(j) = p.job {
            let job = &w.jobs[j];
            let next = job.promotes_to.clone().map(|n| format!(" Next rung: {} (performance {}/9).", n, p.performance)).unwrap_or_default();
            line(format!("Wage ${}/shift, {:02}:00 for {}h, Mon-Fri.{}", job.wage, job.start, job.hours, next), rgb(180, 180, 190));
        }
        let parts: Vec<String> = (0..6).map(|k| format!("{} {}/{}", PART_NAMES[k], p.body.hp[k].max(0), p.body.max[k])).collect();
        for chunk in wrap(&parts.join(", "), 72) {
            line(chunk, rgb(160, 160, 170));
        }
        yy += 1;
        self.text(x + 2, yy, "What people think of you:", rgb(200, 200, 120));
        yy += 1;
        let mut ops: Vec<(i32, usize)> = w
            .people
            .iter()
            .filter(|q| q.alive && !q.is_player && q.opinions.contains_key(&pid))
            .map(|q| (q.opinion(pid), q.id))
            .collect();
        ops.sort_by_key(|(o, _)| -o.abs());
        for (o, id) in ops.iter().take(10) {
            let c = if *o < 0 { rgb(220, 110, 100) } else { rgb(130, 200, 130) };
            self.text(x + 4, yy, &format!("{} {} you. ({})", w.people[*id].name, opinion_word(*o), w.job_name(*id)), c);
            yy += 1;
        }
        if ops.is_empty() {
            self.text(x + 4, yy, "Nobody has an opinion of you yet. That's its own kind of opinion.", rgb(150, 150, 150));
            yy += 1;
        }
        yy += 1;
        self.text(x + 2, yy, "Things you remember:", rgb(200, 200, 120));
        yy += 1;
        for m in p.memories.iter().rev().take(6) {
            let who = m.about.map(|a| w.people[a].name.clone()).unwrap_or_default();
            let s = match m.kind {
                MemKind::Insulted => format!("{} insulted you.", who),
                MemKind::Attacked => format!("{} attacked you.", who),
                MemKind::SawAssault => format!("You saw {} attack someone.", who),
                MemKind::SawMurder => format!("You saw {} kill someone.", who),
                MemKind::SawCorpse => "You saw a body.".into(),
                MemKind::SawFire => format!("You saw {} start a fire.", who),
                MemKind::SawTheft | MemKind::Robbed => format!("{} stole.", who),
                MemKind::HeardRumor => format!("You heard things about {}.", who),
                MemKind::Arrested => format!("{} arrested you.", who),
                MemKind::Complimented => format!("{} was kind to you.", who),
            };
            self.text(x + 4, yy, &s, rgb(170, 170, 170));
            yy += 1;
        }
        let done: Vec<_> = w.objectives.iter().filter(|o| o.done || o.failed).collect();
        if !done.is_empty() {
            yy += 1;
            self.text(x + 2, yy, "Past business:", rgb(200, 120, 230));
            yy += 1;
            for o in done.iter().rev().take(4) {
                let tag = if o.done { "done" } else { "failed" };
                self.text(x + 4, yy, &format!("{} [{}]", o.summary, tag), rgb(170, 140, 190));
                yy += 1;
            }
        }
        let _ = ObjKind::Fires(0);
    }

    fn draw_help(&self) {
        let (x, y) = self.panel(70, 30);
        let lines = [
            ("GREY CITY: how to keep existing", rgb(255, 225, 70)),
            ("", WHITE),
            ("Arrows / numpad / hjklyubn   move (bump people to interact)", WHITE),
            ("Bump a counter or register   buy things (or rob the till)", WHITE),
            ("Bump a payphone              answer it, if it's ringing", WHITE),
            ("t + direction                talk to someone", WHITE),
            ("a + direction                attack", WHITE),
            ("f + direction                set something on fire (needs a lighter)", WHITE),
            (". or s or numpad 5           wait a minute", WHITE),
            ("w / W                        wait 15 minutes / an hour", WHITE),
            ("z                            sleep (best in your own bed)", WHITE),
            ("e                            work (at your station, on shift)", WHITE),
            ("i                            pockets: use items", WHITE),
            ("c                            character, relationships, memories", WHITE),
            ("x                            look around (or hover the mouse)", WHITE),
            ("PgUp / PgDn                  scroll the log", WHITE),
            ("+ / -                        zoom", WHITE),
            ("Esc                          cancel / stop waiting", WHITE),
            ("", WHITE),
            ("Work shifts to get paid. Rent is due every Monday. Miss three", rgb(180, 180, 190)),
            ("shifts and you're fired. People remember what you do and tell", rgb(180, 180, 190)),
            ("each other. Some of them have grudges and money. Some nights,", rgb(180, 180, 190)),
            ("payphones ring.", rgb(180, 180, 190)),
            ("", WHITE),
            ("Your workstation is highlighted on the map.", rgb(180, 180, 190)),
            ("@ yellow you  blue cops  red firefighters  green medics", rgb(180, 180, 190)),
            ("purple: the people who matter.  % the dead.  ^ fire.", rgb(180, 180, 190)),
        ];
        for (k, (s, c)) in lines.iter().enumerate() {
            self.text(x + 2, y + 1 + k as i32, s, *c);
        }
    }

    fn draw_title(&self) {
        let w = self.world.as_ref().unwrap();
        let art = [
            r"   ____                    ____ _ _         ",
            r"  / ___|_ __ ___ _   _    / ___(_) |_ _   _ ",
            r" | |  _| '__/ _ \ | | |  | |   | | __| | | |",
            r" | |_| | | |  __/ |_| |  | |___| | |_| |_| |",
            r"  \____|_|  \___|\__, |   \____|_|\__|\__, |",
            r"                 |___/                |___/ ",
        ];
        for (k, l) in art.iter().enumerate() {
            self.text(4, 2 + k as i32, l, rgb(150, 150, 165));
        }
        self.text(4, 9, "A life simulator. The city doesn't care. It hums.", rgb(200, 120, 230));
        self.text(4, 11, &format!("City seed: {}   (S: new city)", self.seed), rgb(160, 160, 170));
        self.text(4, 13, "Enter / R) Roll a random life (anyone, from dishwasher to vice president)", rgb(255, 225, 70));
        self.text(4, 14, "U)         Start unemployed", rgb(200, 200, 200));
        self.text(4, 16, "Or choose where you start:", rgb(200, 200, 200));
        for (k, j) in title_jobs(w).iter().enumerate() {
            let job = &w.jobs[*j];
            let col = (k / 16) as i32;
            let row = (k % 16) as i32;
            self.text(
                6 + col * 44,
                17 + row,
                &format!("{}) {:<18} ${:>3}/shift", (b'a' + k as u8) as char, job.name, job.wage),
                rgb(170, 200, 230),
            );
        }
    }

    fn draw_dead(&self) {
        let w = self.world.as_ref().unwrap();
        let pid = w.player;
        let p = &w.people[pid];
        let (x, y) = self.panel(70, 20);
        let days = (p.died_turn.saturating_sub(greycity_sim::world::START_TURN)) / 1440;
        let mourners = w.people.iter().filter(|q| q.alive && q.opinion(pid) > 30).count();
        let relieved = w.people.iter().filter(|q| q.alive && q.opinion(pid) < -30).count();
        let lines = [
            ("OBITUARY".to_string(), rgb(255, 225, 70)),
            (String::new(), WHITE),
            (format!("{}, {}, {}.", p.name, p.age, w.job_name(pid)), WHITE),
            (format!("Died on day {} ({}). Lasted {} days.", w.day() + 1, p.cause_of_death.clone().unwrap_or_default(), days), WHITE),
            (format!("Left behind ${}{}.", p.money, if p.rent_owed > 0 { format!(" and ${} in unpaid rent", p.rent_owed) } else { String::new() }), rgb(180, 180, 190)),
            (format!("Arrests: {}. People killed: {}. Fires started: {}.", p.record, p.kills, w.stats.player_fires), rgb(180, 180, 190)),
            (format!("{} people will miss you. {} are relieved.", mourners, relieved), rgb(180, 180, 190)),
            (String::new(), WHITE),
            (
                match mourners {
                    0 => "The funeral is efficient. The flowers are from the landlord, with an invoice.".to_string(),
                    1..=2 => "A few people come to the funeral. One of them takes a sandwich for later.".to_string(),
                    _ => "The funeral is well attended. Someone tells a story about you that isn't true. Everyone laughs.".to_string(),
                },
                rgb(200, 120, 230),
            ),
            (String::new(), WHITE),
            ("The city keeps going. It always does.".to_string(), rgb(150, 150, 165)),
            (String::new(), WHITE),
            ("Press R or Enter to live again.".to_string(), rgb(255, 225, 70)),
        ];
        for (k, (s, c)) in lines.iter().enumerate() {
            for (j, chunk) in wrap(s, 66).iter().enumerate() {
                self.text(x + 2, y + 1 + k as i32 + j as i32, chunk, *c);
            }
        }
    }
}

fn log_color(k: LogKind) -> Color {
    match k {
        LogKind::Normal => rgb(205, 205, 205),
        LogKind::Info => rgb(140, 160, 200),
        LogKind::Good => rgb(130, 210, 130),
        LogKind::Bad => rgb(225, 175, 90),
        LogKind::Danger => rgb(245, 85, 75),
        LogKind::Weird => rgb(200, 125, 235),
        LogKind::Speech => rgb(170, 175, 160),
        LogKind::Offer => rgb(240, 140, 230),
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    for word in s.split(' ') {
        if cur.len() + word.len() + 1 > width && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut app = App::new();
    // GC_DEMO=<minutes>: skip the title, live that long on autopilot, then hand over.
    if let Some(n) = std::env::var("GC_DEMO").ok().and_then(|v| v.parse::<u64>().ok()) {
        let mut w = World::new(app.seed, StartJob::Random);
        w.autopilot = true;
        for _ in 0..n {
            w.step();
        }
        w.autopilot = false;
        w.update_fov();
        app.world = Some(w);
        app.mode = Mode::Play;
    }
    loop {
        app.update();
        app.draw();
        next_frame().await;
    }
}
