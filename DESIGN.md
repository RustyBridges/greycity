# Grey City: design notes

## Pillars
1. **A living world.** The city runs whether or not you are watching. Every
   resident has a job with a shift, a bed (or a bench), money, rent, needs,
   traits, opinions of other people and memories of what they have seen.
2. **Chaos from systems colliding, not scripts.** A drunk line cook starts a
   grease fire; the bar's wooden floor carries it; a coward runs, a brave
   stranger stays; the fire department is off shift; a job opens up because
   its holder burned to death. In SS13 terms: everyone has a job, and the
   station only works while they do it.
3. **Bleak situation, natural hilarity.** The narration is dry. Your life is
   rent, debt and a grey sky. The comedy comes from what the systems do.
4. **The antagonist path comes from the world.** You get no quests. Offers
   come from people: someone who hates a coworker and likes you, a greedy
   acquaintance who knows you're broke, or a payphone that rings at 3am when
   you're at your lowest.

## Decisions made
- **Rust**, with the simulation split from the renderer. The `sim` crate has no
  graphics, is deterministic per seed, and runs headless at about a simulated
  week per second.
- **Plain structs + indices instead of a full ECS** for this first slice. It is
  easier to read and change. An ECS (e.g. `hecs`) can come later if entity types
  multiply (animals, vehicles, items on the ground).
- **Content in RON data files** (`jobs.ron`, `weird.ron`), so you can add jobs
  and strange events without touching code.
- **Turn = 1 minute.** Sleeping, working and waiting fast-forward until done or
  until something important happens near you.
- **The class ladder:** tiers 0-5. Low tiers live in the tenements (two to a
  room), mid tiers in apartments, high tiers in condos. Promotions need
  performance and a vacancy, and vacancies appear when people die, get fired
  or get arrested.

## What exists (vertical slice)
- City generator: 12 lots (tenements, apartments, condos, diner, office tower,
  clinic, precinct + firehouse, bar, corner store, warehouse, park), payphones.
- ~24 jobs, with shifts that wrap past midnight and night workers who sleep by day.
- Needs: hunger, energy, stress, loneliness, drunkenness. Six body parts,
  bleeding, blood loss, knockouts, slow healing in sleep, clinic treatment.
- Economy: wages per completed shift, three strikes and you're fired, weekly
  rent split between roommates, debt, eviction, newcomers moving into empty beds.
- Social: chats, insults, compliments, gossip that spreads crimes as rumours,
  grudges that boil over when someone is drunk or stressed.
- Crime: assault, murder, theft, pickpocketing, arson. Witnesses remember and
  report crimes, cops on shift pursue and arrest, jail time and fines, records
  that block better jobs.
- Fire: fuel-based spread, ash, burning people, the fire department, and
  contractors who eventually rebuild.
- Weird: the hum (people quietly commit arson), doppelgangers, windfalls,
  electrical fires, the payphone and its voice.
- Antagonist objectives: hurt / kill / burn / rob / start fires, with deadlines,
  rewards, and dead employers who can't pay.

## Next steps (roughly in order)
1. **Items in the world:** dropped items, looting bodies, weapons beyond knives.
2. **Bigger city:** more lots, more districts, a few hundred residents. Simulate
   off-screen districts at lower detail.
3. **Families and lineage hooks:** partners, kids and relatives give grudges and
   loyalties more weight.
4. **Factions:** a gang, a cult (the hum, organised), a union, the landlord.
5. **More antagonist roles:** a cult that recruits you, a syndicate with
   escalating jobs, a changeling-style impostor.
6. **Systems:** power grid and blackouts, weather and seasons, disease, the news
   (a daily paper that reports on what you did, inaccurately).
7. **Save/load** (the sim is already deterministic, so serde serialization of
   `World` is straightforward).
