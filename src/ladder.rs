// Puzzle generation and the rules of play

use clap::ValueEnum;

use super::rng::Rng;
use super::words::{one_letter_apart, Dictionary, UNREACHABLE};

#[derive(Copy, Clone, PartialEq, Eq, Debug, ValueEnum)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    /// Range of par (fewest possible steps) for puzzles of this difficulty
    pub fn par_range(self, length: usize) -> (u32, u32) {
        match (length, self) {
            (4, Difficulty::Easy) => (3, 4),
            (4, Difficulty::Medium) => (5, 6),
            (4, Difficulty::Hard) => (7, 9),
            (_, Difficulty::Easy) => (4, 5),
            (_, Difficulty::Medium) => (6, 8),
            (_, Difficulty::Hard) => (9, 12),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Puzzle {
    pub start: usize,
    pub goal: usize,
    pub par: u32,
}

impl Puzzle {
    /// Picks a puzzle whose par is in the difficulty's range. The same seed,
    /// word length and difficulty always give the same puzzle.
    pub fn generate(dict: &Dictionary, difficulty: Difficulty, seed: u64) -> Puzzle {
        let (low, high) = difficulty.par_range(dict.length());
        let mut rng = Rng::new(seed ^ ((dict.length() as u64) << 56) ^ ((difficulty as u64) << 48));
        let common = dict.common();
        loop {
            let start = common[rng.below(common.len())];
            let dist = dict.distances_from(start, true);
            let goals: Vec<usize> = common
                .iter()
                .copied()
                .filter(|&w| (low..=high).contains(&dist[w]))
                .collect();
            if !goals.is_empty() {
                let goal = goals[rng.below(goals.len())];
                return Puzzle {
                    start,
                    goal,
                    par: dist[goal],
                };
            }
        }
    }

    /// A puzzle between two words of the player's choosing
    pub fn custom(dict: &Dictionary, from: &str, to: &str) -> Result<Puzzle, String> {
        let start = dict
            .index_of(from)
            .ok_or(format!("'{}' isn't in the word list", from))?;
        let goal = dict
            .index_of(to)
            .ok_or(format!("'{}' isn't in the word list", to))?;
        if start == goal {
            return Err("Start and goal must be different words".to_string());
        }
        match route_distances(dict, goal)[start] {
            UNREACHABLE => Err(format!("There's no ladder from '{}' to '{}'", from, to)),
            par => Ok(Puzzle { start, goal, par }),
        }
    }
}

/// Fewest steps from every word to the goal, going through familiar words.
/// Falls back to any words for custom puzzles that can't be done that way.
fn route_distances(dict: &Dictionary, goal: usize) -> Vec<u32> {
    let familiar = dict.distances_from(goal, true);
    let has_familiar_route = |w: usize| {
        dict.neighbors(w)
            .iter()
            .any(|&n| familiar[n] != UNREACHABLE)
    };
    if dict.is_familiar(goal) || has_familiar_route(goal) {
        familiar
    } else {
        dict.distances_from(goal, false)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Status {
    Playing,
    Solved,
    Revealed,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StepError {
    WrongLength,
    NotAWord,
    NotOneLetter,
    AlreadyUsed,
}

pub struct Game<'a> {
    dict: &'a Dictionary,
    puzzle: Puzzle,
    steps: Vec<usize>, // the ladder so far, starting with the start word
    to_goal: Vec<u32>, // fewest steps from each word to the goal
    hints: u32,
    status: Status,
}

impl<'a> Game<'a> {
    pub fn new(dict: &'a Dictionary, puzzle: Puzzle) -> Game<'a> {
        Game {
            dict,
            puzzle,
            steps: vec![puzzle.start],
            to_goal: {
                let familiar = route_distances(dict, puzzle.goal);
                if familiar[puzzle.start] == UNREACHABLE {
                    dict.distances_from(puzzle.goal, false)
                } else {
                    familiar
                }
            },
            hints: 0,
            status: Status::Playing,
        }
    }

    pub fn puzzle(&self) -> Puzzle {
        self.puzzle
    }

    pub fn dict(&self) -> &Dictionary {
        self.dict
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn hints(&self) -> u32 {
        self.hints
    }

    /// Words on the ladder so far, starting with the start word
    pub fn steps(&self) -> Vec<&'static str> {
        self.steps.iter().map(|&i| self.dict.word(i)).collect()
    }

    /// Number of moves made
    pub fn step_count(&self) -> u32 {
        self.steps.len() as u32 - 1
    }

    pub fn current(&self) -> &'static str {
        self.dict.word(*self.steps.last().unwrap())
    }

    pub fn goal(&self) -> &'static str {
        self.dict.word(self.puzzle.goal)
    }

    pub fn try_step(&mut self, word: &str) -> Result<Status, StepError> {
        if self.status != Status::Playing {
            return Ok(self.status);
        }
        if word.len() != self.dict.length() {
            return Err(StepError::WrongLength);
        }
        let index = self.dict.index_of(word).ok_or(StepError::NotAWord)?;
        if !one_letter_apart(self.current(), word) {
            return Err(StepError::NotOneLetter);
        }
        if self.steps.contains(&index) {
            return Err(StepError::AlreadyUsed);
        }
        self.steps.push(index);
        if index == self.puzzle.goal {
            self.status = Status::Solved;
        }
        Ok(self.status)
    }

    /// Takes back the last move. Returns false if there's nothing to undo.
    pub fn undo(&mut self) -> bool {
        if self.status != Status::Playing || self.steps.len() < 2 {
            return false;
        }
        self.steps.pop();
        true
    }

    /// The next word on a shortest route from the current word. None if every
    /// shortest route doubles back through a word already on the ladder (the
    /// player needs to undo), or the game is over.
    pub fn hint(&mut self) -> Option<&'static str> {
        if self.status != Status::Playing {
            return None;
        }
        let next = self.next_on_route(*self.steps.last().unwrap(), &self.steps)?;
        self.hints += 1;
        Some(self.dict.word(next))
    }

    pub fn reveal(&mut self) {
        if self.status == Status::Playing {
            self.status = Status::Revealed;
        }
    }

    /// Fewest moves still needed from the current word (None if stuck)
    pub fn moves_left(&self) -> Option<u32> {
        match self.to_goal[*self.steps.last().unwrap()] {
            UNREACHABLE => None,
            n => Some(n),
        }
    }

    /// A shortest ladder from the start word to the goal
    pub fn shortest_route(&self) -> Vec<&'static str> {
        let mut route = vec![self.puzzle.start];
        while let Some(next) = self.next_on_route(*route.last().unwrap(), &[]) {
            route.push(next);
        }
        route.into_iter().map(|i| self.dict.word(i)).collect()
    }

    fn next_on_route(&self, from: usize, exclude: &[usize]) -> Option<usize> {
        let dist = self.to_goal[from];
        if dist == 0 || dist == UNREACHABLE {
            return None;
        }
        let options: Vec<usize> = self
            .dict
            .neighbors(from)
            .iter()
            .copied()
            .filter(|&n| self.to_goal[n] == dist - 1 && !exclude.contains(&n))
            .collect();
        // Prefer common words, then familiar ones, so hints and routes read naturally
        let pick = |ok: &dyn Fn(usize) -> bool| options.iter().copied().find(|&n| ok(n));
        pick(&|n| n == self.puzzle.goal)
            .or_else(|| pick(&|n| self.dict.is_common(n)))
            .or_else(|| pick(&|n| self.dict.is_familiar(n)))
            .or(options.first().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cold_warm(dict: &Dictionary) -> Game<'_> {
        Game::new(dict, Puzzle::custom(dict, "cold", "warm").unwrap())
    }

    #[test]
    fn test_custom_puzzle() {
        let dict = Dictionary::new(4).unwrap();
        let puzzle = Puzzle::custom(&dict, "cold", "warm").unwrap();
        assert_eq!(puzzle.par, 4);
        assert!(Puzzle::custom(&dict, "cold", "cold").is_err());
        assert!(Puzzle::custom(&dict, "cold", "xqzz")
            .unwrap_err()
            .contains("word list"));
    }

    #[test]
    fn test_solve_at_par() {
        let dict = Dictionary::new(4).unwrap();
        let mut game = cold_warm(&dict);
        for word in ["cord", "word", "ward"] {
            assert_eq!(game.try_step(word), Ok(Status::Playing));
        }
        assert_eq!(game.try_step("warm"), Ok(Status::Solved));
        assert_eq!(game.step_count(), 4);
        assert_eq!(game.steps(), vec!["cold", "cord", "word", "ward", "warm"]);
    }

    #[test]
    fn test_step_errors() {
        let dict = Dictionary::new(4).unwrap();
        let mut game = cold_warm(&dict);
        assert_eq!(game.try_step("col"), Err(StepError::WrongLength));
        assert_eq!(game.try_step("colx"), Err(StepError::NotAWord));
        assert_eq!(game.try_step("card"), Err(StepError::NotOneLetter));
        game.try_step("cord").unwrap();
        assert_eq!(game.try_step("cold"), Err(StepError::AlreadyUsed));
        assert_eq!(game.step_count(), 1);
    }

    #[test]
    fn test_undo() {
        let dict = Dictionary::new(4).unwrap();
        let mut game = cold_warm(&dict);
        assert!(!game.undo());
        game.try_step("cord").unwrap();
        assert!(game.undo());
        assert_eq!(game.current(), "cold");
    }

    #[test]
    fn test_hint_follows_a_shortest_route() {
        let dict = Dictionary::new(4).unwrap();
        let mut game = cold_warm(&dict);
        while game.status() == Status::Playing {
            let before = game.moves_left().unwrap();
            let hint = game.hint().unwrap();
            game.try_step(hint).unwrap();
            assert_eq!(game.moves_left().unwrap(), before - 1);
        }
        assert_eq!(game.step_count(), 4);
        assert_eq!(game.hints(), 4);
    }

    #[test]
    fn test_hint_skips_words_already_used() {
        let dict = Dictionary::new(4).unwrap();
        let mut game = cold_warm(&dict);
        // cold -> cord, then step sideways and back towards cold's neighbours
        game.try_step("cord").unwrap();
        game.try_step("card").unwrap();
        if let Some(hint) = game.hint() {
            assert!(!game.steps().contains(&hint));
            assert!(game.try_step(hint).is_ok());
        }
    }

    #[test]
    fn test_routes_use_familiar_words() {
        for length in [4, 5] {
            let dict = Dictionary::new(length).unwrap();
            for seed in 0..20 {
                let p = Puzzle::generate(&dict, Difficulty::Hard, seed);
                let game = Game::new(&dict, p);
                let route = game.shortest_route();
                assert_eq!(route.len() as u32, p.par + 1);
                for w in route {
                    assert!(dict.is_familiar(dict.index_of(w).unwrap()), "{w}");
                }
            }
        }
    }

    #[test]
    fn test_custom_puzzle_with_rare_words() {
        let dict = Dictionary::new(4).unwrap();
        // "coll" is accepted but rare; it can still be a custom goal
        let p = Puzzle::custom(&dict, "warm", "coll").unwrap();
        let route = Game::new(&dict, p).shortest_route();
        assert_eq!(route.len() as u32, p.par + 1);
        assert_eq!(route.last(), Some(&"coll"));
        // Only the goal itself is rare
        for w in &route[..route.len() - 1] {
            assert!(dict.is_familiar(dict.index_of(w).unwrap()), "{w}");
        }
    }

    #[test]
    fn test_shortest_route() {
        let dict = Dictionary::new(4).unwrap();
        let game = cold_warm(&dict);
        let route = game.shortest_route();
        assert_eq!(route.first(), Some(&"cold"));
        assert_eq!(route.last(), Some(&"warm"));
        assert_eq!(route.len(), 5);
        assert!(route.windows(2).all(|w| one_letter_apart(w[0], w[1])));
    }

    #[test]
    fn test_reveal_ends_game() {
        let dict = Dictionary::new(4).unwrap();
        let mut game = cold_warm(&dict);
        game.reveal();
        assert_eq!(game.status(), Status::Revealed);
        assert!(game.hint().is_none());
        assert!(!game.undo());
    }

    #[test]
    fn test_generated_puzzles_match_difficulty() {
        for length in [4, 5] {
            let dict = Dictionary::new(length).unwrap();
            for difficulty in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard] {
                let (low, high) = difficulty.par_range(length);
                for seed in 0..5 {
                    let p = Puzzle::generate(&dict, difficulty, seed);
                    assert!(
                        (low..=high).contains(&p.par),
                        "{length} {difficulty:?} par {}",
                        p.par
                    );
                    assert!(dict.is_common(p.start) && dict.is_common(p.goal));
                    assert_eq!(dict.distances_from(p.start, true)[p.goal], p.par);
                }
            }
        }
    }

    #[test]
    fn test_same_seed_same_puzzle() {
        let dict = Dictionary::new(4).unwrap();
        let a = Puzzle::generate(&dict, Difficulty::Medium, 1234);
        let b = Puzzle::generate(&dict, Difficulty::Medium, 1234);
        let c = Puzzle::generate(&dict, Difficulty::Hard, 1234);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
