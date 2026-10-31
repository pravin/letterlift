use clap::Parser;

use super::ladder::Difficulty;

/// Climb from one word to another, changing one letter at a time
#[derive(Parser, Clone, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Config {
    /// Play today's daily puzzle: 4 letters, medium, the same for everyone (no internet needed)
    #[arg(long, short)]
    pub daily: bool,

    /// Puzzle difficulty
    #[arg(long, value_enum, default_value_t = Difficulty::Medium)]
    pub difficulty: Difficulty,

    /// Word length: 4 (classic) or 5 (longer ladders)
    #[arg(long, short, default_value_t = 4, value_parser = clap::value_parser!(u8).range(4..=5))]
    pub length: u8,

    /// Replay a puzzle by its number (shown in the game). The number includes
    /// the word length and difficulty, so friends get exactly the same puzzle
    #[arg(long, short, value_name = "NUMBER", conflicts_with = "daily")]
    pub puzzle: Option<u64>,

    /// Start word for your own puzzle (use with --to)
    #[arg(long, requires = "to", conflicts_with_all = ["daily", "puzzle"])]
    pub from: Option<String>,

    /// Goal word for your own puzzle (use with --from)
    #[arg(long, requires = "from")]
    pub to: Option<String>,

    /// Show statistics
    #[arg(long, short)]
    pub stats: bool,

    /// Reset all statistics
    #[arg(long)]
    pub reset_stats: bool,

    /// Disable colours. Also honours the NO_COLOR environment variable
    #[arg(long)]
    pub no_color: bool,
}

impl Config {
    pub fn parse_args() -> Self {
        Config::parse()
    }

    /// Whether to draw with colours: off with --no-color or a non-empty NO_COLOR (https://no-color.org)
    pub fn use_color(&self) -> bool {
        !self.no_color && std::env::var_os("NO_COLOR").map_or(true, |v| v.is_empty())
    }

    /// Word length for this game: a custom puzzle's words set it
    pub fn word_length(&self) -> usize {
        match &self.from {
            Some(word) => word.len(),
            None => self.length as usize,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(number) = self.puzzle {
            PuzzleNumber::decode(number)?;
        }
        if let (Some(from), Some(to)) = (&self.from, &self.to) {
            for word in [from, to] {
                if !word.chars().all(|c| c.is_ascii_alphabetic()) {
                    return Err(format!("'{}' must contain only letters", word));
                }
                if !(4..=5).contains(&word.len()) {
                    return Err(format!("'{}' must be 4 or 5 letters long", word));
                }
            }
            if from.len() != to.len() {
                return Err("--from and --to must be the same length".to_string());
            }
        }
        Ok(())
    }
}

/// A shareable puzzle number: the random seed plus the word length and difficulty
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PuzzleNumber {
    pub seed: u64,
    pub length: usize,
    pub difficulty: Difficulty,
}

const DIFFICULTIES: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard];

impl PuzzleNumber {
    pub fn encode(self) -> u64 {
        let kind = (self.length as u64 - 4) * 3 + self.difficulty as u64;
        self.seed * 10 + kind
    }

    pub fn decode(number: u64) -> Result<PuzzleNumber, String> {
        let kind = number % 10;
        if kind > 5 {
            return Err(format!("{} isn't a valid puzzle number", number));
        }
        Ok(PuzzleNumber {
            seed: number / 10,
            length: 4 + (kind / 3) as usize,
            difficulty: DIFFICULTIES[(kind % 3) as usize],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Config, clap::Error> {
        Config::try_parse_from(std::iter::once("letterlift").chain(args.iter().copied()))
    }

    #[test]
    fn test_defaults() {
        let c = parse(&[]).unwrap();
        assert_eq!(c.word_length(), 4);
        assert_eq!(c.difficulty, Difficulty::Medium);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_length_range() {
        assert!(parse(&["--length", "5"]).is_ok());
        assert!(parse(&["--length", "6"]).is_err());
    }

    #[test]
    fn test_custom_puzzle_args() {
        assert!(parse(&["--from", "cold"]).is_err()); // needs --to
        let c = parse(&["--from", "Bread", "--to", "toast"]).unwrap();
        assert_eq!(c.word_length(), 5);
        assert!(c.validate().is_ok());
        assert!(parse(&["--from", "cold", "--to", "bread"])
            .unwrap()
            .validate()
            .is_err());
        assert!(parse(&["--from", "c0ld", "--to", "warm"])
            .unwrap()
            .validate()
            .is_err());
    }

    #[test]
    fn test_puzzle_number_round_trip() {
        for length in [4, 5] {
            for difficulty in DIFFICULTIES {
                let n = PuzzleNumber {
                    seed: 482913,
                    length,
                    difficulty,
                };
                assert_eq!(PuzzleNumber::decode(n.encode()), Ok(n));
            }
        }
        assert!(PuzzleNumber::decode(1239).is_err());
        assert!(parse(&["--puzzle", "1239"]).unwrap().validate().is_err());
    }

    #[test]
    fn test_daily_conflicts_with_puzzle() {
        assert!(parse(&["--daily", "--puzzle", "12"]).is_err());
    }
}
