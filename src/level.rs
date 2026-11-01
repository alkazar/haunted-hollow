//! The levels (game modes): the same ride, track and rules, themed differently.
//! `--level` picks one. Each level builds its own world (`world::halloween`,
//! `world::christmas`), models (`mesh::Models::new`) and music (`audio::Bank::build`);
//! this module has its names, words and colours.

use crate::world::{Doors, Zone};
use glam::Vec3;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Level {
    /// Haunted Hollow: a haunted house and the Pumpkin King.
    #[default]
    Halloween,
    /// Holly Hollow: Santa's workshop and the Snow King.
    Christmas,
}

impl Level {
    pub const ALL: [Level; 2] = [Level::Halloween, Level::Christmas];

    /// The level named on the command line (a few aliases are accepted).
    pub fn parse(s: &str) -> Option<Level> {
        match s.to_ascii_lowercase().as_str() {
            "halloween" | "haunted" | "spooky" => Some(Level::Halloween),
            "christmas" | "xmas" | "holly" | "winter" => Some(Level::Christmas),
            _ => None,
        }
    }

    /// The name used on the command line and in file names.
    pub fn id(self) -> &'static str {
        match self {
            Level::Halloween => "halloween",
            Level::Christmas => "christmas",
        }
    }

    pub fn theme(self) -> &'static Theme {
        match self {
            Level::Halloween => &HALLOWEEN,
            Level::Christmas => &CHRISTMAS,
        }
    }

    /// The box of the world that is stored as blocks: (lowest corner, highest corner
    /// exclusive). Below it is solid stone, above it open sky.
    pub fn bounds(self) -> ([i32; 3], [i32; 3]) {
        match self {
            Level::Halloween => ([-40, -32, -32], [40, 44, 300]),
            Level::Christmas => ([-64, -40, -48], [64, 44, 344]),
        }
    }

    /// The grand doors behind the boss, which open once he's beaten. The ride leaves through
    /// them along +z.
    pub fn doors(self) -> Doors {
        match self {
            Level::Halloween => Doors { x: 0.0, floor: -17.0, z: 214.5 },
            Level::Christmas => Doors { x: 0.0, floor: -24.0, z: 268.5 },
        }
    }

    /// Where the boss starts, hidden below the floor (he rises 16.5 m and sways about it).
    pub fn boss_home(self) -> Vec3 {
        match self {
            Level::Halloween => Vec3::new(0.0, -26.0, 200.0),
            Level::Christmas => Vec3::new(0.0, -33.0, 254.0),
        }
    }
}

/// One part of a ride, announced with a banner as the cart enters it.
pub struct Section {
    pub name: &'static str,
    /// Fog colour, fog start and end, and the cart lantern's strength.
    pub fog: ([f32; 3], (f32, f32), f32),
    /// Out under the sky (clouds, and snow at Christmas).
    pub outdoors: bool,
    /// No whoosh when the cart enters it (the big drops).
    pub quiet: bool,
}

const fn section(name: &'static str, fog: ([f32; 3], (f32, f32), f32), outdoors: bool, quiet: bool) -> Section {
    Section { name, fog, outdoors, quiet }
}

/// A level's words and colours.
pub struct Theme {
    pub title: &'static str,
    pub subtitle: &'static str,
    /// Window title.
    pub window: &'static str,
    /// Big headings (title, results, high scores) and the countdown.
    pub accent: [f32; 4],
    /// The line under the title.
    pub accent2: [f32; 4],
    /// Background of the results, initials and high-score screens.
    pub backdrop: [f32; 4],
    /// The letter grid's key border.
    pub key_border: [f32; 4],
    pub results: &'static str,
    pub boss: &'static str,
    pub boss_beaten: &'static str,
    pub boss_first_taunt: &'static str,
    pub taunts: [&'static str; 5],
    pub summon: &'static str,
    /// Boss health bar: background, and its colour when empty and when full.
    pub boss_bar: [f32; 4],
    pub boss_fill: ([f32; 3], [f32; 3]),
    /// Pitch of the boss's laugh.
    pub laugh_pitch: f32,
    /// Award per enemy kind (in `Kind` order), then the boss, then the fallback.
    pub awards: [&'static str; 10],
    /// Confetti that targets burst into.
    pub confetti: [[f32; 3]; 6],
    /// The parts of the ride, in order (`Zone` indexes them).
    pub sections: &'static [Section],
}

impl Theme {
    pub fn section(&self, z: Zone) -> &Section {
        &self.sections[(z.0 as usize).min(self.sections.len() - 1)]
    }

    pub fn zone_name(&self, z: Zone) -> &'static str {
        self.section(z).name
    }

    pub fn zone_fog(&self, z: Zone) -> ([f32; 3], (f32, f32), f32) {
        self.section(z).fog
    }
}

const HOUSE_FOG: ([f32; 3], (f32, f32), f32) = ([0.04, 0.03, 0.06], (18.0, 64.0), 0.95);

static HALLOWEEN: Theme = Theme {
    title: "HAUNTED HOLLOW",
    subtitle: "THE SPOOKY COASTER RIDE",
    window: "Haunted Hollow",
    accent: [1.0, 0.55, 0.08, 1.0],
    accent2: [0.75, 0.55, 1.0, 1.0],
    backdrop: [0.04, 0.02, 0.07, 0.94],
    key_border: [0.5, 0.32, 0.7, 1.0],
    results: "WHAT A SPOOKTACULAR RIDE!",
    boss: "THE PUMPKIN KING",
    boss_beaten: "YOU BEAT THE PUMPKIN KING!",
    boss_first_taunt: "MWAHAHA!",
    taunts: ["MWAHAHA!", "TRICK OR TREAT!", "YOU CAN'T CATCH ME!", "OOH, THAT TICKLES!", "BOO!"],
    summon: "GET THEM, MY BATS!",
    boss_bar: [0.1, 0.03, 0.12, 0.85],
    boss_fill: ([1.0, 0.45, 0.05], [1.0, 0.75, 0.05]),
    laugh_pitch: 1.0,
    awards: [
        "PUMPKIN POPPER",
        "GHOST CATCHER",
        "BAT BOPPER",
        "BOOKWORM",
        "BONE BONKER",
        "SPIDER SPOTTER",
        "CANDY CHAMP",
        "ART CRITIC",
        "BOSS BUSTER",
        "SPOOKY STAR",
    ],
    confetti: [[1.0, 0.55, 0.1], [0.7, 0.3, 1.0], [0.4, 1.0, 0.3], [1.0, 0.9, 0.2], [1.0, 0.45, 0.75], [1.0, 1.0, 1.0]],
    sections: &[
        section("THE GRAVEYARD", ([0.07, 0.06, 0.15], (30.0, 130.0), 0.5), true, false),
        section("THE GRAND FOYER", HOUSE_FOG, false, false),
        section("THE HAUNTED LIBRARY", HOUSE_FOG, false, false),
        section("THE DINING HALL", HOUSE_FOG, false, false),
        section("HOLD ON TIGHT!", ([0.02, 0.04, 0.03], (10.0, 44.0), 0.9), false, true),
        section("THE CREEPY CRYPT", ([0.03, 0.06, 0.05], (14.0, 50.0), 0.9), false, false),
        section("THE PUMPKIN KING'S LAIR", ([0.015, 0.005, 0.025], (24.0, 80.0), 0.45), false, false),
    ],
};

const NIGHT_FOG: ([f32; 3], (f32, f32), f32) = ([0.1, 0.12, 0.22], (34.0, 140.0), 0.5);

static CHRISTMAS: Theme = Theme {
    title: "HOLLY HOLLOW",
    subtitle: "THE FROSTY CHRISTMAS COASTER",
    window: "Holly Hollow",
    accent: [1.0, 0.28, 0.25, 1.0],
    accent2: [0.45, 1.0, 0.55, 1.0],
    backdrop: [0.02, 0.05, 0.1, 0.94],
    key_border: [0.35, 0.6, 0.85, 1.0],
    results: "WHAT A MERRY RIDE!",
    boss: "THE SNOW KING",
    boss_beaten: "YOU MELTED THE SNOW KING!",
    boss_first_taunt: "HO HO HO!",
    taunts: ["HO HO HO!", "BRRR, YOU'RE COLD!", "YOU CAN'T MELT ME!", "OOH, THAT TICKLES!", "SNOWBALL FIGHT!"],
    summon: "GET THEM, MY SNOWMEN!",
    boss_bar: [0.02, 0.06, 0.12, 0.85],
    boss_fill: ([0.3, 0.5, 1.0], [0.55, 0.9, 1.0]),
    laugh_pitch: 0.72,
    awards: [
        "SNOWMAN SMASHER",
        "COOKIE CATCHER",
        "ACE PILOT",
        "PRESENT POPPER",
        "NUTCRACKER CRACKER",
        "BAUBLE BOPPER",
        "SUPERSTAR",
        "WREATH WRANGLER",
        "FROST BUSTER",
        "JOLLY STAR",
    ],
    confetti: [[1.0, 0.2, 0.2], [0.2, 0.9, 0.35], [1.0, 1.0, 1.0], [1.0, 0.85, 0.2], [0.55, 0.85, 1.0], [1.0, 0.5, 0.6]],
    sections: &[
        section("THE SNOWY FOREST", ([0.08, 0.1, 0.19], (26.0, 110.0), 0.55), true, false),
        section("WINTERBERRY VILLAGE", NIGHT_FOG, true, false),
        section("OVER THE ROOFTOPS", ([0.12, 0.14, 0.26], (40.0, 160.0), 0.4), true, false),
        section("DOWN THE CHIMNEY!", ([0.05, 0.03, 0.03], (10.0, 40.0), 0.9), false, true),
        section("THE TOY FACTORY", ([0.07, 0.04, 0.035], (22.0, 72.0), 0.85), false, false),
        section("THE FROZEN LAKE", NIGHT_FOG, true, false),
        section("HOLD ON TIGHT!", ([0.03, 0.06, 0.1], (10.0, 44.0), 0.9), false, true),
        section("THE CRYSTAL CAVERN", ([0.04, 0.09, 0.14], (16.0, 56.0), 0.8), false, false),
        section("THE SNOW KING'S THRONE", ([0.03, 0.05, 0.1], (24.0, 80.0), 0.45), false, false),
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_parse_from_their_names() {
        for l in Level::ALL {
            assert_eq!(Level::parse(l.id()), Some(l));
        }
        assert_eq!(Level::parse("XMAS"), Some(Level::Christmas));
        assert_eq!(Level::parse("easter"), None);
    }
}
