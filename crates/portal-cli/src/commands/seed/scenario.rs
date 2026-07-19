//! Seed scenario definitions.
//!
//! All entity IDs are derived deterministically via UUID v5 so that
//! re-running the seed is idempotent (`ON CONFLICT DO NOTHING`).

use uuid::Uuid;

/// Fixed namespace for deterministic seed UUIDs.
const SEED_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
]);

/// Derive a deterministic UUID from a seed key.
pub fn seed_uuid(key: &str) -> Uuid {
    Uuid::new_v5(&SEED_NAMESPACE, key.as_bytes())
}

/// Shared password for all seed users (dev-only).
pub const SEED_PASSWORD: &str = "SeedPassword123!";

// ---------------------------------------------------------------------------
// Persona definitions
// ---------------------------------------------------------------------------

pub struct Persona {
    pub key: &'static str,
    pub username: &'static str,
    pub email: &'static str,
    pub display_name: &'static str,
    pub country_code: &'static str,
    pub is_admin: bool,
}

impl Persona {
    pub fn user_id(&self) -> Uuid {
        seed_uuid(&format!("user:{}", self.key))
    }
    pub fn player_id(&self) -> Uuid {
        seed_uuid(&format!("player:{}", self.key))
    }
}

pub const PERSONAS: &[Persona] = &[
    Persona {
        key: "admin",
        username: "seed_admin",
        email: "admin@seed.local",
        display_name: "Alice Admin",
        country_code: "US",
        is_admin: true,
    },
    Persona {
        key: "organizer",
        username: "seed_organizer",
        email: "organizer@seed.local",
        display_name: "Bob Organizer",
        country_code: "US",
        is_admin: false,
    },
    Persona {
        key: "captain_alpha",
        username: "seed_captain_alpha",
        email: "captain_alpha@seed.local",
        display_name: "Charlie Alpha",
        country_code: "SE",
        is_admin: false,
    },
    Persona {
        key: "captain_bravo",
        username: "seed_captain_bravo",
        email: "captain_bravo@seed.local",
        display_name: "Diana Bravo",
        country_code: "DE",
        is_admin: false,
    },
    Persona {
        key: "captain_charlie",
        username: "seed_captain_charlie",
        email: "captain_charlie@seed.local",
        display_name: "Erik Charlie",
        country_code: "FR",
        is_admin: false,
    },
    Persona {
        key: "captain_delta",
        username: "seed_captain_delta",
        email: "captain_delta@seed.local",
        display_name: "Fiona Delta",
        country_code: "UK",
        is_admin: false,
    },
    // Team Alpha players (SE)
    Persona {
        key: "player_1",
        username: "seed_player_1",
        email: "player1@seed.local",
        display_name: "George",
        country_code: "SE",
        is_admin: false,
    },
    Persona {
        key: "player_2",
        username: "seed_player_2",
        email: "player2@seed.local",
        display_name: "Hank",
        country_code: "SE",
        is_admin: false,
    },
    Persona {
        key: "player_3",
        username: "seed_player_3",
        email: "player3@seed.local",
        display_name: "Iris",
        country_code: "SE",
        is_admin: false,
    },
    Persona {
        key: "player_4",
        username: "seed_player_4",
        email: "player4@seed.local",
        display_name: "Johan",
        country_code: "SE",
        is_admin: false,
    },
    // Team Bravo players (DE)
    Persona {
        key: "player_5",
        username: "seed_player_5",
        email: "player5@seed.local",
        display_name: "Klaus",
        country_code: "DE",
        is_admin: false,
    },
    Persona {
        key: "player_6",
        username: "seed_player_6",
        email: "player6@seed.local",
        display_name: "Lena",
        country_code: "DE",
        is_admin: false,
    },
    Persona {
        key: "player_7",
        username: "seed_player_7",
        email: "player7@seed.local",
        display_name: "Moritz",
        country_code: "DE",
        is_admin: false,
    },
    Persona {
        key: "player_8",
        username: "seed_player_8",
        email: "player8@seed.local",
        display_name: "Nina",
        country_code: "DE",
        is_admin: false,
    },
    // Team Charlie players (FR)
    Persona {
        key: "player_9",
        username: "seed_player_9",
        email: "player9@seed.local",
        display_name: "Olivier",
        country_code: "FR",
        is_admin: false,
    },
    Persona {
        key: "player_10",
        username: "seed_player_10",
        email: "player10@seed.local",
        display_name: "Pierre",
        country_code: "FR",
        is_admin: false,
    },
    Persona {
        key: "player_11",
        username: "seed_player_11",
        email: "player11@seed.local",
        display_name: "Quentin",
        country_code: "FR",
        is_admin: false,
    },
    Persona {
        key: "player_12",
        username: "seed_player_12",
        email: "player12@seed.local",
        display_name: "Renee",
        country_code: "FR",
        is_admin: false,
    },
    // Team Delta players (UK)
    Persona {
        key: "player_13",
        username: "seed_player_13",
        email: "player13@seed.local",
        display_name: "Simon",
        country_code: "UK",
        is_admin: false,
    },
    Persona {
        key: "player_14",
        username: "seed_player_14",
        email: "player14@seed.local",
        display_name: "Tom",
        country_code: "UK",
        is_admin: false,
    },
    Persona {
        key: "player_15",
        username: "seed_player_15",
        email: "player15@seed.local",
        display_name: "Uma",
        country_code: "UK",
        is_admin: false,
    },
    Persona {
        key: "player_16",
        username: "seed_player_16",
        email: "player16@seed.local",
        display_name: "Victor",
        country_code: "UK",
        is_admin: false,
    },
    Persona {
        key: "spectator",
        username: "seed_spectator",
        email: "spectator@seed.local",
        display_name: "Sam Spectator",
        country_code: "US",
        is_admin: false,
    },
    Persona {
        key: "newbie",
        username: "seed_newbie",
        email: "newbie@seed.local",
        display_name: "Tara Newbie",
        country_code: "US",
        is_admin: false,
    },
];

// ---------------------------------------------------------------------------
// Team definitions
// ---------------------------------------------------------------------------

pub struct TeamDef {
    pub key: &'static str,
    pub name: &'static str,
    pub tag: &'static str,
    pub captain_key: &'static str,
    pub member_keys: &'static [&'static str],
}

impl TeamDef {
    pub fn team_id(&self) -> Uuid {
        seed_uuid(&format!("team:{}", self.key))
    }
}

pub const TEAMS: &[TeamDef] = &[
    TeamDef {
        key: "alpha",
        name: "Team Alpha",
        tag: "ALFA",
        captain_key: "captain_alpha",
        member_keys: &["player_1", "player_2", "player_3", "player_4"],
    },
    TeamDef {
        key: "bravo",
        name: "Team Bravo",
        tag: "BRVO",
        captain_key: "captain_bravo",
        member_keys: &["player_5", "player_6", "player_7", "player_8"],
    },
    TeamDef {
        key: "charlie",
        name: "Team Charlie",
        tag: "CHRL",
        captain_key: "captain_charlie",
        member_keys: &["player_9", "player_10", "player_11", "player_12"],
    },
    TeamDef {
        key: "delta",
        name: "Team Delta",
        tag: "DLTA",
        captain_key: "captain_delta",
        member_keys: &["player_13", "player_14", "player_15", "player_16"],
    },
];

// ---------------------------------------------------------------------------
// Rating profiles (base_rating, trend_per_entry) per team player
// ---------------------------------------------------------------------------

/// (persona_key, base_rating, trend_per_entry)
pub const RATING_PROFILES: &[(&str, i32, i32)] = &[
    // Team Alpha — High elo (Pink/Red)
    ("captain_alpha", 25_500, 80),
    ("player_1", 23_000, 60),
    ("player_2", 26_800, 40),
    ("player_3", 21_500, 100),
    ("player_4", 24_200, 50),
    // Team Bravo — Mid-high elo (Purple/Pink)
    ("captain_bravo", 19_500, 60),
    ("player_5", 17_200, 80),
    ("player_6", 20_800, 30),
    ("player_7", 18_000, 50),
    ("player_8", 16_500, 70),
    // Team Charlie — Mid elo (Blue/Purple)
    ("captain_charlie", 14_500, 40),
    ("player_9", 12_000, 60),
    ("player_10", 15_800, 20),
    ("player_11", 13_200, 50),
    ("player_12", 11_500, 30),
    // Team Delta — Low elo (Grey/Light Blue)
    ("captain_delta", 6_500, 30),
    ("player_13", 4_000, 50),
    ("player_14", 7_200, 20),
    ("player_15", 5_500, 40),
    ("player_16", 3_200, 60),
];

/// Number of history entries per player.
pub const RATING_HISTORY_COUNT: usize = 15;

/// Deterministic wobble that avoids RNG. Returns a small +/- offset.
pub fn deterministic_wobble(player_idx: usize, entry_idx: usize) -> i32 {
    const MAX_FLUCTUATION: i32 = 400;
    let sign = if (player_idx + entry_idx).is_multiple_of(3) {
        -1
    } else {
        1
    };
    let factor = match entry_idx % 5 {
        0 => 3,
        1 => 8,
        2 => 1,
        3 => 6,
        _ => 5,
    };
    sign * (MAX_FLUCTUATION * factor / 10)
}

// ---------------------------------------------------------------------------
// Availability windows: (persona_key, day_of_week, start_h, start_m, end_h, end_m, is_preferred, timezone)
// day_of_week: 0=Sun, 1=Mon, 2=Tue, 3=Wed, 4=Thu, 5=Fri, 6=Sat
//
// Design: Each team has 2 shared "team night" slots (is_preferred=true) where
// all 5 members are available. Some players have extra personal slots.
// Cross-team overlaps exist for tournament matchups:
//   Alpha vs Delta overlap on Tuesday 19:00-22:00 UTC
//   Bravo vs Charlie overlap on Wednesday 19:00-22:00 UTC
// ---------------------------------------------------------------------------

/// One availability window entry:
/// (persona_key, day_of_week, start_h, start_m, end_h, end_m, is_preferred, timezone).
pub type AvailabilityWindowSpec = (&'static str, u8, u8, u8, u8, u8, bool, &'static str);

pub const AVAILABILITY_WINDOWS: &[AvailabilityWindowSpec] = &[
    // --- Team Alpha (SE) shared: Tue 18-22, Thu 18-22 ---
    ("captain_alpha", 2, 18, 0, 22, 0, true, "Europe/Stockholm"),
    ("player_1", 2, 18, 0, 22, 0, true, "Europe/Stockholm"),
    ("player_2", 2, 18, 0, 22, 0, true, "Europe/Stockholm"),
    ("player_3", 2, 18, 0, 22, 0, true, "Europe/Stockholm"),
    ("player_4", 2, 18, 0, 22, 0, true, "Europe/Stockholm"),
    ("captain_alpha", 4, 18, 0, 22, 0, true, "Europe/Stockholm"),
    ("player_1", 4, 18, 0, 22, 0, true, "Europe/Stockholm"),
    ("player_2", 4, 18, 0, 22, 0, true, "Europe/Stockholm"),
    ("player_3", 4, 18, 0, 22, 0, true, "Europe/Stockholm"),
    ("player_4", 4, 18, 0, 22, 0, true, "Europe/Stockholm"),
    // Alpha extras
    ("captain_alpha", 6, 14, 0, 20, 0, false, "Europe/Stockholm"),
    ("player_1", 3, 19, 0, 22, 0, false, "Europe/Stockholm"),
    ("player_2", 6, 15, 0, 21, 0, false, "Europe/Stockholm"),
    // --- Team Bravo (DE) shared: Mon 18-22, Wed 18-22 ---
    ("captain_bravo", 1, 18, 0, 22, 0, true, "Europe/Berlin"),
    ("player_5", 1, 18, 0, 22, 0, true, "Europe/Berlin"),
    ("player_6", 1, 18, 0, 22, 0, true, "Europe/Berlin"),
    ("player_7", 1, 18, 0, 22, 0, true, "Europe/Berlin"),
    ("player_8", 1, 18, 0, 22, 0, true, "Europe/Berlin"),
    ("captain_bravo", 3, 18, 0, 22, 0, true, "Europe/Berlin"),
    ("player_5", 3, 18, 0, 22, 0, true, "Europe/Berlin"),
    ("player_6", 3, 18, 0, 22, 0, true, "Europe/Berlin"),
    ("player_7", 3, 18, 0, 22, 0, true, "Europe/Berlin"),
    ("player_8", 3, 18, 0, 22, 0, true, "Europe/Berlin"),
    // Bravo extras
    ("captain_bravo", 6, 16, 0, 22, 0, false, "Europe/Berlin"),
    ("player_5", 2, 19, 0, 22, 0, false, "Europe/Berlin"),
    ("player_6", 5, 18, 0, 22, 0, false, "Europe/Berlin"),
    // --- Team Charlie (FR) shared: Wed 19-23, Sun 14-18 ---
    ("captain_charlie", 3, 19, 0, 23, 0, true, "Europe/Paris"),
    ("player_9", 3, 19, 0, 23, 0, true, "Europe/Paris"),
    ("player_10", 3, 19, 0, 23, 0, true, "Europe/Paris"),
    ("player_11", 3, 19, 0, 23, 0, true, "Europe/Paris"),
    ("player_12", 3, 19, 0, 23, 0, true, "Europe/Paris"),
    ("captain_charlie", 0, 14, 0, 18, 0, true, "Europe/Paris"),
    ("player_9", 0, 14, 0, 18, 0, true, "Europe/Paris"),
    ("player_10", 0, 14, 0, 18, 0, true, "Europe/Paris"),
    ("player_11", 0, 14, 0, 18, 0, true, "Europe/Paris"),
    ("player_12", 0, 14, 0, 18, 0, true, "Europe/Paris"),
    // Charlie extras
    ("captain_charlie", 1, 19, 0, 22, 0, false, "Europe/Paris"),
    ("player_9", 4, 19, 0, 22, 0, false, "Europe/Paris"),
    ("player_10", 6, 14, 0, 18, 0, false, "Europe/Paris"),
    // --- Team Delta (UK) shared: Tue 19-23, Sat 14-18 ---
    ("captain_delta", 2, 19, 0, 23, 0, true, "Europe/London"),
    ("player_13", 2, 19, 0, 23, 0, true, "Europe/London"),
    ("player_14", 2, 19, 0, 23, 0, true, "Europe/London"),
    ("player_15", 2, 19, 0, 23, 0, true, "Europe/London"),
    ("player_16", 2, 19, 0, 23, 0, true, "Europe/London"),
    ("captain_delta", 6, 14, 0, 18, 0, true, "Europe/London"),
    ("player_13", 6, 14, 0, 18, 0, true, "Europe/London"),
    ("player_14", 6, 14, 0, 18, 0, true, "Europe/London"),
    ("player_15", 6, 14, 0, 18, 0, true, "Europe/London"),
    ("player_16", 6, 14, 0, 18, 0, true, "Europe/London"),
    // Delta extras
    ("captain_delta", 4, 19, 0, 22, 0, false, "Europe/London"),
    ("player_13", 3, 18, 0, 21, 0, false, "Europe/London"),
    ("player_14", 0, 15, 0, 19, 0, false, "Europe/London"),
];

// ---------------------------------------------------------------------------
// Entity IDs (non-persona)
// ---------------------------------------------------------------------------

pub fn league_id() -> Uuid {
    seed_uuid("league:competitive_cs2")
}

pub fn premier_league_id() -> Uuid {
    seed_uuid("league:cs2_premier")
}

pub fn tournament_id() -> Uuid {
    seed_uuid("tournament:weekly_cup_1")
}

pub fn tournament_stage_id() -> Uuid {
    seed_uuid("tournament_stage:weekly_cup_1_main")
}

// Tournament 2 (self-scheduled)
pub fn tournament_2_id() -> Uuid {
    seed_uuid("tournament:cs2_showdown_2")
}

pub fn tournament_2_stage_id() -> Uuid {
    seed_uuid("tournament_stage:cs2_showdown_2_main")
}

pub fn tournament_2_bracket_id() -> Uuid {
    seed_uuid("tournament_bracket:cs2_showdown_2_main")
}

pub fn tournament_2_registration_id(team_key: &str) -> Uuid {
    seed_uuid(&format!("tournament2_reg:{team_key}"))
}

pub fn tournament_2_match_id(position: &str) -> Uuid {
    seed_uuid(&format!("tournament2_match:{position}"))
}

pub fn tournament_2_veto_session_id(position: &str) -> Uuid {
    seed_uuid(&format!("tournament2_veto:{position}"))
}

pub fn tournament_2_map_pool_id() -> Uuid {
    seed_uuid("tournament2_map_pool")
}

/// Average seed rating for a team (used for tournament seeding).
pub fn team_seed_rating(team_key: &str) -> i32 {
    // Map team keys to approximate average ratings
    match team_key {
        "alpha" => 25_000,
        "bravo" => 18_000,
        "charlie" => 14_000,
        "delta" => 5_000,
        _ => 10_000,
    }
}

/// Find a persona by key. Panics if not found (compile-time data).
pub fn persona(key: &str) -> &'static Persona {
    PERSONAS
        .iter()
        .find(|p| p.key == key)
        .unwrap_or_else(|| panic!("Unknown persona key: {key}"))
}

/// Collect all user IDs for reset.
pub fn all_user_ids() -> Vec<Uuid> {
    PERSONAS.iter().map(Persona::user_id).collect()
}

/// Collect all player IDs for reset.
pub fn all_player_ids() -> Vec<Uuid> {
    PERSONAS.iter().map(Persona::player_id).collect()
}
