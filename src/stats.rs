use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// How a finished puzzle went
pub struct Outcome<'a> {
    pub solved: bool,
    pub steps: u32,
    pub par: u32,
    pub hints: u32,
    /// For the daily puzzle: today's and yesterday's dates (YYYY-MM-DD)
    pub daily: Option<(&'a str, &'a str)>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Stats {
    pub played: u32,
    pub solved: u32,
    pub at_par: u32,      // solved in exactly par steps
    pub extra_steps: u32, // total steps over par, across solved puzzles
    pub hints: u32,
    pub current_streak: u32, // puzzles solved in a row without revealing
    pub max_streak: u32,
    pub daily_last: Option<String>, // date of the last daily puzzle played
    pub daily_streak: u32,          // daily puzzles solved on consecutive days
    pub daily_max_streak: u32,
}

impl Stats {
    pub fn record(&mut self, outcome: &Outcome) {
        self.played += 1;
        self.hints += outcome.hints;
        if outcome.solved {
            self.solved += 1;
            self.current_streak += 1;
            self.max_streak = self.max_streak.max(self.current_streak);
            if outcome.steps <= outcome.par {
                self.at_par += 1;
            }
            self.extra_steps += outcome.steps.saturating_sub(outcome.par);
        } else {
            self.current_streak = 0;
        }

        if let Some((today, yesterday)) = outcome.daily {
            if outcome.solved {
                let continues = self.daily_last.as_deref() == Some(yesterday);
                self.daily_streak = if continues { self.daily_streak + 1 } else { 1 };
                self.daily_max_streak = self.daily_max_streak.max(self.daily_streak);
            } else {
                self.daily_streak = 0;
            }
            self.daily_last = Some(today.to_string());
        }
    }

    pub fn played_daily_on(&self, date: &str) -> bool {
        self.daily_last.as_deref() == Some(date)
    }

    pub fn solve_rate(&self) -> f32 {
        if self.played == 0 {
            0.0
        } else {
            self.solved as f32 * 100.0 / self.played as f32
        }
    }

    pub fn average_over_par(&self) -> f32 {
        if self.solved == 0 {
            0.0
        } else {
            self.extra_steps as f32 / self.solved as f32
        }
    }

    /// Plain-text summary, shared by the stats screen and `--stats`
    pub fn lines(&self) -> Vec<String> {
        vec![
            "STATISTICS".to_string(),
            String::new(),
            format!("Puzzles played:     {}", self.played),
            format!("Solved:             {:.0}%", self.solve_rate()),
            format!("Solved at par:      {}", self.at_par),
            format!("Avg. over par:      {:.1}", self.average_over_par()),
            format!("Hints used:         {}", self.hints),
            format!("Current streak:     {}", self.current_streak),
            format!("Best streak:        {}", self.max_streak),
            String::new(),
            format!("Daily streak:       {}", self.daily_streak),
            format!("Best daily streak:  {}", self.daily_max_streak),
        ]
    }
}

#[derive(Default)]
pub struct StatsManager {
    stats: Stats,
    file_path: Option<PathBuf>, // None = in-memory only (no writable data dir)
}

impl StatsManager {
    /// Loads stats from the platform data directory (e.g. ~/.local/share/letterlift/
    /// on Linux). Never fails: without a data directory, stats are kept in memory.
    pub fn new() -> Self {
        match Self::default_path() {
            Some(path) => Self::with_path(path),
            None => Self::default(),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        let stats = match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| {
                // Keep the unreadable file around rather than silently overwriting it
                let _ = fs::rename(&path, path.with_extension("json.bak"));
                Stats::default()
            }),
            Err(_) => Stats::default(),
        };
        Self {
            stats,
            file_path: Some(path),
        }
    }

    pub fn default_path() -> Option<PathBuf> {
        Some(
            dirs::data_dir()?
                .join(env!("CARGO_PKG_NAME"))
                .join("stats.json"),
        )
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    pub fn record(&mut self, outcome: &Outcome) -> Result<(), Box<dyn std::error::Error>> {
        self.stats.record(outcome);
        self.save()
    }

    pub fn reset(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.stats = Stats::default();
        self.save()
    }

    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(path) = &self.file_path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Write to a temp file then rename, so a crash can't leave a half-written file
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(&self.stats)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(solved: bool, steps: u32, par: u32) -> Outcome<'static> {
        Outcome {
            solved,
            steps,
            par,
            hints: 0,
            daily: None,
        }
    }

    #[test]
    fn test_record_solved_and_revealed() {
        let mut s = Stats::default();
        s.record(&outcome(true, 5, 5));
        s.record(&outcome(true, 7, 5));
        s.record(&outcome(false, 2, 5));
        assert_eq!((s.played, s.solved, s.at_par, s.extra_steps), (3, 2, 1, 2));
        assert_eq!((s.current_streak, s.max_streak), (0, 2));
        assert_eq!(s.average_over_par(), 1.0);
    }

    #[test]
    fn test_daily_streak() {
        let mut s = Stats::default();
        let day = |today, yesterday, solved| Outcome {
            daily: Some((today, yesterday)),
            ..outcome(solved, 5, 5)
        };
        s.record(&day("2026-11-01", "2026-10-31", true));
        s.record(&day("2026-11-02", "2026-11-01", true));
        assert_eq!(s.daily_streak, 2);
        assert!(s.played_daily_on("2026-11-02"));
        // Skipping a day restarts the streak
        s.record(&day("2026-11-04", "2026-11-03", true));
        assert_eq!((s.daily_streak, s.daily_max_streak), (1, 2));
        s.record(&day("2026-11-05", "2026-11-04", false));
        assert_eq!(s.daily_streak, 0);
    }

    #[test]
    fn test_manager_persists_and_backs_up_corrupt_file() {
        let dir = std::env::temp_dir().join(format!("letterlift-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("stats.json");

        let mut manager = StatsManager::with_path(path.clone());
        manager.record(&outcome(true, 4, 4)).unwrap();
        assert_eq!(StatsManager::with_path(path.clone()).stats().solved, 1);

        fs::write(&path, "not json").unwrap();
        assert_eq!(StatsManager::with_path(path.clone()).stats().played, 0);
        assert_eq!(
            fs::read_to_string(path.with_extension("json.bak")).unwrap(),
            "not json"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
