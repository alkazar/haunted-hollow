//! The high-score table: the top ten rides, each with up to three initials, kept in a
//! small text file ("SCORE INITIALS" per line) so it survives between sessions.

use std::path::Path;

pub const TABLE_SIZE: usize = 10;
pub const MAX_INITIALS: usize = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub score: u32,
    pub initials: String,
}

#[derive(Clone, Debug, Default)]
pub struct HighScores {
    /// Best first.
    pub entries: Vec<Entry>,
}

/// Keeps only letters and digits, upper-cased, at most three of them.
pub fn clean_initials(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).take(MAX_INITIALS).collect()
}

impl HighScores {
    /// Reads the table, or starts an empty one if the file is missing or unreadable.
    pub fn load(path: &Path) -> HighScores {
        let Ok(text) = std::fs::read_to_string(path) else { return HighScores::default() };
        let mut entries: Vec<Entry> = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let score = parts.next()?.parse().ok()?;
                let initials = clean_initials(parts.next().unwrap_or(""));
                (!initials.is_empty()).then_some(Entry { score, initials })
            })
            .collect();
        entries.sort_by(|a, b| b.score.cmp(&a.score));
        entries.truncate(TABLE_SIZE);
        HighScores { entries }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text: String = self.entries.iter().map(|e| format!("{} {}\n", e.score, e.initials)).collect();
        std::fs::write(path, text)
    }

    /// Which of this ride's scores make the table, considering them all together
    /// (two riders can't both take the last place). Returns their indices into `scores`.
    pub fn qualifiers(&self, scores: &[u32]) -> Vec<usize> {
        let mut all: Vec<(u32, Option<usize>)> = self.entries.iter().map(|e| (e.score, None)).collect();
        all.extend(scores.iter().enumerate().filter(|(_, &s)| s > 0).map(|(i, &s)| (s, Some(i))));
        // Stable sort: on a tie the score already in the table stays ahead.
        all.sort_by(|a, b| b.0.cmp(&a.0));
        all.into_iter().take(TABLE_SIZE).filter_map(|(_, i)| i).collect()
    }

    /// Adds an entry below any equal scores and returns its rank (0 = best), or None
    /// if it didn't make the table.
    pub fn insert(&mut self, score: u32, initials: &str) -> Option<usize> {
        let at = self.entries.iter().position(|e| e.score < score).unwrap_or(self.entries.len());
        if at >= TABLE_SIZE {
            return None;
        }
        self.entries.insert(at, Entry { score, initials: clean_initials(initials) });
        self.entries.truncate(TABLE_SIZE);
        Some(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(scores: &[u32]) -> HighScores {
        HighScores { entries: scores.iter().map(|&s| Entry { score: s, initials: "AAA".into() }).collect() }
    }

    #[test]
    fn riders_only_qualify_for_places_left_after_each_other() {
        let t = table(&[1000, 900, 800, 700, 600, 500, 400, 300, 200, 100]);
        // Two 150s can't both beat the 100 for the last place: the first rider gets it.
        assert_eq!(t.qualifiers(&[150, 150]), vec![0]);
        assert_eq!(t.qualifiers(&[50, 1100]), vec![1]);
        assert!(t.qualifiers(&[0]).is_empty(), "a score of zero never qualifies");
    }

    #[test]
    fn inserting_keeps_the_table_sorted_and_capped() {
        let mut t = table(&[900, 800, 700, 600, 500, 400, 300, 200, 100, 50]);
        assert_eq!(t.insert(650, "bob!"), Some(3));
        assert_eq!(t.entries[3], Entry { score: 650, initials: "BOB".into() });
        assert_eq!(t.entries.len(), TABLE_SIZE);
        assert_eq!(t.entries.last().unwrap().score, 100);
        assert_eq!(t.insert(10, "X"), None);
    }

    #[test]
    fn saves_and_loads_the_table() {
        let dir = std::env::temp_dir().join(format!("haunted-hollow-test-{}", std::process::id()));
        let path = dir.join("highscores.txt");
        let mut t = HighScores::default();
        t.insert(1200, "AL");
        t.insert(3400, "ZED");
        t.save(&path).unwrap();
        let back = HighScores::load(&path);
        assert_eq!(back.entries, vec![Entry { score: 3400, initials: "ZED".into() }, Entry { score: 1200, initials: "AL".into() }]);
        std::fs::remove_dir_all(dir).ok();
        assert!(HighScores::load(Path::new("/nonexistent/x")).entries.is_empty());
    }
}
