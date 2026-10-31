// Terminal UI: draws the game with crossterm and handles keyboard input.
// The whole screen is redrawn from game state on every event, which keeps
// rendering simple and makes terminal resizes work for free.
//
// The goal word sits at the top and the ladder grows upwards towards it,
// starting from the start word at the bottom.

use std::cmp::Ordering;
use std::io::{self, Stdout, Write};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};

use super::config::{Config, PuzzleNumber};
use super::daily;
use super::ladder::{Game, Puzzle, Status, StepError};
use super::rng::Rng;
use super::stats::{Outcome, StatsManager};
use super::words::Dictionary;

const TITLE: &str = env!("CARGO_PKG_NAME");
const MIN_WIDTH: u16 = 60;
const CHAR_SPACING: u16 = 4;
/// Rows used by everything except the ladder: title, subtitle, goal and the
/// gaps between them, then a gap, message, footer and keys line
const FIXED_ROWS: u16 = 10;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TileStyle {
    Blank, // empty box waiting for a letter
    Typed, // letter typed in the input row
    Goal,  // the goal word
    Match, // letter already matching the goal in this position
    Plain, // any other letter on the ladder
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MessageKind {
    Info,
    Error,
    Success,
}

type Message = Option<(String, MessageKind)>;

enum Action {
    Letter(char),
    Backspace,
    Enter,
    Hint,
    Reveal,
    Quit,
    Other,
    Redraw,
}

/// What kind of puzzle is being played, for the subtitle and stats
#[derive(Clone, Copy)]
enum Mode {
    Daily(chrono::NaiveDate),
    Numbered(PuzzleNumber),
    Custom,
}

/// Restores the terminal to its normal state
fn restore_terminal() {
    let _ = execute!(
        io::stdout(),
        ResetColor,
        cursor::Show,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}

struct Ui {
    out: Stdout,
    color: bool,
}

impl Ui {
    /// Switches the terminal to full-screen raw mode. The terminal is restored
    /// when the Ui is dropped, and also if the program panics.
    fn new(color: bool) -> io::Result<Ui> {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            default_hook(info);
        }));

        terminal::enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(Ui { out, color })
    }

    fn read_action(&self) -> io::Result<Action> {
        loop {
            let key = match event::read()? {
                Event::Key(key) => key,
                Event::Resize(_, _) => return Ok(Action::Redraw),
                _ => continue,
            };
            // Windows reports key releases too; only act on presses
            if key.kind == KeyEventKind::Release {
                continue;
            }
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            return Ok(match key.code {
                KeyCode::Char('c') | KeyCode::Char('d') if ctrl => Action::Quit,
                KeyCode::Esc => Action::Quit,
                KeyCode::Enter => Action::Enter,
                KeyCode::Backspace | KeyCode::Delete => Action::Backspace,
                KeyCode::Char(_) if ctrl => Action::Other,
                KeyCode::Char('?') => Action::Hint,
                KeyCode::Char('!') => Action::Reveal,
                KeyCode::Char(c) if c.is_ascii_alphabetic() => {
                    Action::Letter(c.to_ascii_lowercase())
                }
                KeyCode::Char(_) => Action::Other,
                _ => continue,
            });
        }
    }

    fn print_at(&mut self, x: u16, y: u16, text: &str, cols: u16) -> io::Result<()> {
        let room = cols.saturating_sub(x) as usize;
        let text: String = text.chars().take(room).collect();
        queue!(self.out, cursor::MoveTo(x, y), Print(text))
    }

    fn print_centered(&mut self, y: u16, text: &str, cols: u16) -> io::Result<()> {
        let x = cols.saturating_sub(text.chars().count() as u16) / 2;
        self.print_at(x, y, text, cols)
    }

    fn draw_tile(&mut self, x: u16, y: u16, ch: char, style: TileStyle) -> io::Result<()> {
        let ch = ch.to_ascii_uppercase();
        if !self.color {
            // Without colours, brackets carry the meaning (see legend)
            let text = match style {
                TileStyle::Blank => " _ ".to_string(),
                TileStyle::Goal => format!("<{}>", ch),
                TileStyle::Match => format!("[{}]", ch),
                TileStyle::Typed | TileStyle::Plain => format!(" {} ", ch),
            };
            return queue!(self.out, cursor::MoveTo(x, y), Print(text));
        }

        let (fg, bg) = match style {
            TileStyle::Blank => (Color::Black, Color::Grey),
            TileStyle::Typed => (Color::White, Color::DarkBlue),
            TileStyle::Goal => (Color::Black, Color::DarkCyan),
            TileStyle::Match => (Color::Black, Color::DarkGreen),
            TileStyle::Plain => (Color::White, Color::DarkGrey),
        };
        queue!(
            self.out,
            cursor::MoveTo(x, y),
            SetForegroundColor(fg),
            SetBackgroundColor(bg),
            Print(format!(" {} ", ch)),
            ResetColor
        )
    }

    fn draw_word(&mut self, x: u16, y: u16, tiles: &[(char, TileStyle)]) -> io::Result<()> {
        for (i, &(ch, style)) in tiles.iter().enumerate() {
            self.draw_tile(x + i as u16 * CHAR_SPACING, y, ch, style)?;
        }
        Ok(())
    }

    fn draw_title(&mut self, y: u16, cols: u16) -> io::Result<()> {
        let width = TITLE.len() as u16 * CHAR_SPACING - 1;
        if width > cols || !self.color {
            let spaced: String = TITLE
                .to_uppercase()
                .chars()
                .map(|c| format!("{c} "))
                .collect();
            return self.print_centered(y, spaced.trim_end(), cols);
        }
        let x = (cols - width) / 2;
        let pattern = [TileStyle::Match, TileStyle::Plain, TileStyle::Goal];
        for (i, ch) in TITLE.chars().enumerate() {
            self.draw_tile(
                x + i as u16 * CHAR_SPACING,
                y,
                ch,
                pattern[i % pattern.len()],
            )?;
        }
        Ok(())
    }

    fn draw_message(&mut self, y: u16, message: &Message, cols: u16) -> io::Result<()> {
        let Some((text, kind)) = message else {
            return Ok(());
        };
        if self.color {
            let (fg, bg) = match kind {
                MessageKind::Info => (Color::Reset, Color::Reset),
                MessageKind::Error => (Color::White, Color::DarkRed),
                MessageKind::Success => (Color::Black, Color::DarkGreen),
            };
            queue!(self.out, SetForegroundColor(fg), SetBackgroundColor(bg))?;
        }
        self.print_centered(y, &format!(" {} ", text), cols)?;
        queue!(self.out, ResetColor)
    }

    fn draw_game(&mut self, screen: &Screen) -> io::Result<()> {
        queue!(self.out, ResetColor, terminal::Clear(ClearType::All))?;
        let (cols, rows) = terminal::size()?;
        let game = screen.game;
        let length = game.dict().length() as u16;
        let legend = !self.color as u16;

        // Ladder entries from top to bottom: the input row, then newest step to start word
        let playing = game.status() == Status::Playing;
        let steps = game.steps();
        let entries = steps.len() as u16 + playing as u16;
        let wanted = entries.max(game.puzzle().par as u16 + 2) * 2 - 1;
        let available = rows.saturating_sub(FIXED_ROWS + legend);
        if cols < MIN_WIDTH || available < 3 {
            let text = format!(
                "Terminal too small: {}x{} (need {}x{}). Resize, or press Esc to quit.",
                cols,
                rows,
                MIN_WIDTH,
                FIXED_ROWS + legend + 3
            );
            self.print_at(0, 0, &text, cols)?;
            return self.out.flush();
        }
        let area = wanted.min(available);
        let gap = if entries * 2 - 1 <= area { 2 } else { 1 };
        let capacity = if gap == 2 { area.div_ceil(2) } else { area };

        let top = (rows - (FIXED_ROWS + legend + area)) / 2;
        let word_x = cols.saturating_sub(length * CHAR_SPACING - 1) / 2;
        let label_x = word_x.saturating_sub(8);

        self.draw_title(top, cols)?;
        self.print_centered(top + 2, &screen.subtitle, cols)?;

        // Goal word
        let goal = game.goal();
        let goal_y = top + 4;
        let tiles: Vec<_> = goal.chars().map(|c| (c, TileStyle::Goal)).collect();
        self.draw_word(word_x, goal_y, &tiles)?;
        self.print_at(label_x, goal_y, "  goal", cols)?;

        // Ladder, anchored to the bottom of its area
        let mut rows_top_down: Vec<Vec<(char, TileStyle)>> = Vec::new();
        if playing {
            let mut typed: Vec<_> = screen
                .input
                .chars()
                .map(|c| (c, TileStyle::Typed))
                .collect();
            typed.resize(length as usize, (' ', TileStyle::Blank));
            rows_top_down.push(typed);
        }
        for word in steps.iter().rev() {
            let tiles = word
                .chars()
                .zip(goal.chars())
                .map(|(c, g)| {
                    (
                        c,
                        if c == g {
                            TileStyle::Match
                        } else {
                            TileStyle::Plain
                        },
                    )
                })
                .collect();
            rows_top_down.push(tiles);
        }
        let shown = rows_top_down.len().min(capacity as usize);
        let bottom = goal_y + 2 + area - 1;
        for (k, tiles) in rows_top_down.iter().take(shown).enumerate() {
            let y = bottom - (shown - 1 - k) as u16 * gap;
            self.draw_word(word_x, y, tiles)?;
        }
        if shown == rows_top_down.len() {
            self.print_at(label_x, bottom, " start", cols)?;
        }

        // Message, footer and keys
        let below = goal_y + 2 + area + 1;
        self.draw_message(below, &screen.message, cols)?;
        self.print_centered(below + 1, &screen.footer, cols)?;
        self.print_centered(below + 2, &screen.keys, cols)?;
        if legend == 1 {
            let text = "<X> goal   [X] matches the goal";
            self.print_centered(below + 3, text, cols)?;
        }
        self.out.flush()
    }

    fn show_stats(&mut self, lines: &[String]) -> io::Result<()> {
        loop {
            queue!(self.out, ResetColor, terminal::Clear(ClearType::All))?;
            let (cols, rows) = terminal::size()?;
            let mut lines = lines.to_vec();
            lines.push(String::new());
            lines.push("Press any key to go back".to_string());
            if lines.len() >= rows as usize {
                lines.retain(|l| !l.is_empty());
            }
            let width = lines.iter().map(|l| l.len()).max().unwrap_or(0) as u16;
            let x = cols.saturating_sub(width) / 2;
            for (i, line) in lines.iter().enumerate() {
                self.print_at(x, 1 + i as u16, line, cols)?;
            }
            self.out.flush()?;
            if !matches!(self.read_action()?, Action::Redraw) {
                return Ok(());
            }
        }
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Everything drawn for one frame
struct Screen<'a, 'g> {
    game: &'a Game<'g>,
    subtitle: String,
    input: &'a str,
    message: Message,
    footer: String,
    keys: String,
}

fn subtitle(mode: Mode, game: &Game) -> String {
    let par = game.puzzle().par;
    match mode {
        Mode::Daily(date) => format!("Daily #{} | par {}", daily::number(date), par),
        Mode::Numbered(n) => format!(
            "Puzzle {} | {} | par {}",
            n.encode(),
            n.difficulty.name(),
            par
        ),
        Mode::Custom => format!("Your puzzle | par {}", par),
    }
}

fn step_error(error: StepError, word: &str, current: &str) -> String {
    let (word, current) = (word.to_uppercase(), current.to_uppercase());
    match error {
        StepError::WrongLength => "Not enough letters".to_string(),
        StepError::NotAWord => format!("{} isn't in the word list", word),
        StepError::NotOneLetter => format!("Change exactly one letter of {}", current),
        StepError::AlreadyUsed => format!("{} is already on your ladder", word),
    }
}

/// A line to share after a daily or numbered puzzle, printed when the game exits
fn share_line(mode: Mode, game: &Game) -> Option<String> {
    let route = format!(
        "{} -> {}",
        game.steps()[0].to_uppercase(),
        game.goal().to_uppercase()
    );
    let result = match game.status() {
        Status::Solved => format!("in {} steps (par {})", game.step_count(), game.puzzle().par),
        _ => "revealed".to_string(),
    };
    match mode {
        Mode::Daily(date) => Some(format!(
            "{} daily #{}: {} {}",
            TITLE,
            daily::number(date),
            route,
            result
        )),
        Mode::Numbered(n) => Some(format!(
            "{} puzzle {}: {} {}  (play it: {} --puzzle {})",
            TITLE,
            n.encode(),
            route,
            result,
            TITLE,
            n.encode()
        )),
        Mode::Custom => None,
    }
}

/// Plays one puzzle. Returns false if the player quit.
fn play(
    ui: &mut Ui,
    dict: &Dictionary,
    puzzle: Puzzle,
    mode: Mode,
    stats_manager: &mut StatsManager,
    share: &mut Option<String>,
) -> io::Result<bool> {
    let mut game = Game::new(dict, puzzle);
    let subtitle = subtitle(mode, &game);
    let mut input = String::new();
    let mut message: Message = None;

    let today = daily::today();
    let today_str = today.format("%Y-%m-%d").to_string();
    let replaying_daily =
        matches!(mode, Mode::Daily(_)) && stats_manager.stats().played_daily_on(&today_str);
    if replaying_daily {
        message = Some((
            "Already played today, so this one won't count".to_string(),
            MessageKind::Info,
        ));
    }

    // Play until solved or revealed
    while game.status() == Status::Playing {
        let footer = format!("Steps {} | par {}", game.step_count(), game.puzzle().par);
        let keys = "Enter: climb | Bksp: undo | ?: hint | !: reveal | Esc: quit".to_string();
        ui.draw_game(&Screen {
            game: &game,
            subtitle: subtitle.clone(),
            input: &input,
            message: message.clone(),
            footer,
            keys,
        })?;
        let action = ui.read_action()?;
        if !matches!(action, Action::Redraw) {
            message = None;
        }
        match action {
            Action::Quit => return Ok(false),
            Action::Letter(c) if input.len() < dict.length() => input.push(c),
            Action::Backspace if input.is_empty() => {
                if game.undo() {
                    message = Some(("Took back a step".to_string(), MessageKind::Info));
                }
            }
            Action::Backspace => {
                input.pop();
            }
            Action::Enter => match game.try_step(&input) {
                Ok(_) => input.clear(),
                Err(e) => {
                    message = Some((step_error(e, &input, game.current()), MessageKind::Error))
                }
            },
            Action::Hint => {
                message = Some(match game.hint() {
                    Some(word) => (format!("Try {}", word.to_uppercase()), MessageKind::Info),
                    None => (
                        "No hint from here. Undo a step with Backspace".to_string(),
                        MessageKind::Info,
                    ),
                });
            }
            Action::Reveal => game.reveal(),
            _ => {}
        }
    }

    // Finished: record stats and show the result
    let solved = game.status() == Status::Solved;
    let (steps, par, hints) = (game.step_count(), game.puzzle().par, game.hints());
    let yesterday = daily::yesterday(today).format("%Y-%m-%d").to_string();
    let saved = if replaying_daily {
        Ok(())
    } else {
        stats_manager.record(&Outcome {
            solved,
            steps,
            par,
            hints,
            daily: matches!(mode, Mode::Daily(_))
                .then_some((today_str.as_str(), yesterday.as_str())),
        })
    };
    *share = share_line(mode, &game);

    let hint_note = match hints {
        0 => String::new(),
        1 => " with 1 hint".to_string(),
        n => format!(" with {} hints", n),
    };
    let mut result = match (solved, steps.cmp(&par)) {
        (true, Ordering::Less) => (
            format!("Solved in {} steps{}. Under par!", steps, hint_note),
            MessageKind::Success,
        ),
        (true, Ordering::Equal) => (
            format!("Solved in {} steps{}. Par!", steps, hint_note),
            MessageKind::Success,
        ),
        (true, Ordering::Greater) => (
            format!("Solved in {} steps (par {}){}", steps, par, hint_note),
            MessageKind::Info,
        ),
        (false, _) => (
            "Revealed. Better luck next time!".to_string(),
            MessageKind::Info,
        ),
    };
    if let Err(e) = saved {
        result.0 = format!("{} (stats not saved: {})", result.0, e);
    }
    let route = if solved && steps <= par {
        String::new()
    } else {
        let words: Vec<String> = game
            .shortest_route()
            .iter()
            .map(|w| w.to_uppercase())
            .collect();
        format!("Shortest: {}", words.join(" > "))
    };

    loop {
        ui.draw_game(&Screen {
            game: &game,
            subtitle: subtitle.clone(),
            input: "",
            message: Some(result.clone()),
            footer: route.clone(),
            keys: "N: new puzzle | S: stats | Q: quit".to_string(),
        })?;
        match ui.read_action()? {
            Action::Letter('n') | Action::Enter => return Ok(true),
            Action::Letter('s') => ui.show_stats(&stats_manager.stats().lines())?,
            Action::Letter('q') | Action::Quit => return Ok(false),
            _ => {}
        }
    }
}

/// Runs puzzles until the player quits. Returns a line to share, if any.
pub fn run(config: &Config, stats_manager: &mut StatsManager) -> Result<Option<String>, String> {
    let numbered = config.puzzle.map(PuzzleNumber::decode).transpose()?;
    let length = match numbered {
        _ if config.daily => daily::LENGTH,
        Some(n) => n.length,
        None => config.word_length(),
    };
    let dict = Dictionary::new(length)?;
    let random = |difficulty| {
        let number = PuzzleNumber {
            seed: Rng::fresh_seed(),
            length,
            difficulty,
        };
        (
            Puzzle::generate(&dict, difficulty, number.seed),
            Mode::Numbered(number),
        )
    };

    let mut next = if config.daily {
        let date = daily::today();
        (
            Puzzle::generate(&dict, daily::DIFFICULTY, daily::seed(date)),
            Mode::Daily(date),
        )
    } else if let (Some(from), Some(to)) = (&config.from, &config.to) {
        let puzzle = Puzzle::custom(&dict, &from.to_lowercase(), &to.to_lowercase())?;
        (puzzle, Mode::Custom)
    } else if let Some(number) = numbered {
        (
            Puzzle::generate(&dict, number.difficulty, number.seed),
            Mode::Numbered(number),
        )
    } else {
        random(config.difficulty)
    };

    let mut ui = Ui::new(config.use_color()).map_err(|e| e.to_string())?;
    let mut share = None;
    loop {
        let (puzzle, mode) = next;
        if !play(&mut ui, &dict, puzzle, mode, stats_manager, &mut share)
            .map_err(|e| e.to_string())?
        {
            return Ok(share);
        }
        next = random(config.difficulty);
    }
}
