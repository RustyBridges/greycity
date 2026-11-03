//! Run the city with no window, the player on autopilot, and print everything
//! that happens. Useful for balancing and for proving the sim is deterministic.
//!
//!   cargo run -p greycity-sim --bin headless -- --seed 7 --days 3 [--quiet]

use greycity_sim::world::{StartJob, World};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<u64>().ok());
    let seed = get("--seed").unwrap_or(1);
    let days = get("--days").unwrap_or(3);
    let quiet = args.iter().any(|a| a == "--quiet");

    let mut w = World::new(seed, StartJob::Random);
    w.autopilot = true;
    let t0 = std::time::Instant::now();
    let end = w.turn + days * 1440;
    let mut printed = 0;
    while w.turn < end {
        w.step();
        if !quiet {
            for e in &w.log[printed.min(w.log.len())..] {
                let d = e.turn / 1440 + 1;
                println!("[day {} {:02}:{:02}] {:?}: {}", d, (e.turn % 1440) / 60, e.turn % 60, e.kind, e.text);
            }
        }
        printed = w.log.len();
    }
    let alive = w.people.iter().filter(|p| p.alive).count();
    let jailed = w.people.iter().filter(|p| p.alive && p.jailed_until > w.turn).count();
    let homeless = w.people.iter().filter(|p| p.alive && p.home.is_none()).count();
    let employed = w.people.iter().filter(|p| p.alive && p.job.is_some()).count();
    println!("---");
    println!("seed {} | {} days simulated in {:.2?}", seed, days, t0.elapsed());
    println!("population {} alive / {} ever | employed {} | homeless {} | in jail {}", alive, w.people.len(), employed, homeless, jailed);
    println!("{:?}", w.stats);
    for b in &w.map.buildings {
        if b.burned > 0 {
            println!("  {} burned tiles: {}", b.name, b.burned);
        }
    }
}
