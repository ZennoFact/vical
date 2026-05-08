use std::env;
use std::fs;
use std::io;
use std::io::Write;
use std::iter::Peekable;
use std::path::PathBuf;
use std::str::Chars;
use std::time::{Duration, Instant};

use arboard::Clipboard;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListDirection, ListItem, ListState, Paragraph},
    Terminal,
};
use serde::Deserialize;

#[derive(Clone, Copy)]
struct Theme {
    accent_bg: Color,
    accent_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            accent_bg: Color::Rgb(173, 216, 230),
            accent_fg: Color::Black,
        }
    }
}

#[derive(Deserialize)]
struct SettingFile {
    accent_color: RgbSetting,
    accent_text_color: Option<RgbSetting>,
}

#[derive(Deserialize)]
struct RgbSetting {
    r: u8,
    g: u8,
    b: u8,
}

impl RgbSetting {
    fn to_color(&self) -> Color {
        Color::Rgb(self.r, self.g, self.b)
    }
}

fn config_file_path() -> io::Result<PathBuf> {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("vical")
        .join("setting.toml"))
}

fn default_setting_toml() -> &'static str {
    "[accent_color]\nr = 173\ng = 216\nb = 230\n\n[accent_text_color]\nr = 0\ng = 0\nb = 0\n"
}

fn load_theme() -> Theme {
    let path = match config_file_path() {
        Ok(path) => path,
        Err(_) => return Theme::default(),
    };

    if !path.exists() {
        if let Some(parent) = path.parent() {
            if fs::create_dir_all(parent).is_err() {
                return Theme::default();
            }
        }
        if fs::write(&path, default_setting_toml()).is_err() {
            return Theme::default();
        }
    }

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return Theme::default(),
    };

    let parsed: SettingFile = match toml::from_str(&content) {
        Ok(parsed) => parsed,
        Err(_) => return Theme::default(),
    };

    Theme {
        accent_bg: parsed.accent_color.to_color(),
        accent_fg: parsed
            .accent_text_color
            .as_ref()
            .map(RgbSetting::to_color)
            .unwrap_or(Color::Black),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        if let Err(e) = run_tui() {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    let mut copy_result = false;
    let mut positional: Vec<String> = Vec::new();
    for arg in args.iter().skip(1) {
        if arg == "-c" || arg == "--copy" {
            copy_result = true;
        } else {
            positional.push(arg.clone());
        }
    }

    if positional.is_empty() {
        eprintln!("Error: no expression or mode specified");
        eprintln!("Use --help for usage information");
        std::process::exit(1);
    }

    if positional[0] == "-h" || positional[0] == "--help" {
        print_cli_help();
        return;
    }

    if positional[0] == "--add" || positional[0] == "-a" {
        if let Err(e) = run_add_mode() {
            eprintln!("Add mode error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if positional[0] == "--sub" || positional[0] == "-s" {
        if let Err(e) = run_sub_mode() {
            eprintln!("Sub mode error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if positional[0] == "--pow" || positional[0] == "-p" {
        if let Err(e) = run_pow_mode() {
            eprintln!("Pow mode error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if positional[0] == "--binary" || positional[0] == "-b" {
        if let Err(e) = run_binary_mode(&positional, copy_result) {
            eprintln!("Binary mode error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if positional[0] == "--from-binary" || positional[0] == "-fb" {
        if let Err(e) = run_from_binary_mode(&positional, copy_result) {
            eprintln!("From-binary mode error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    let expr: String = positional.concat();
    let mut parser = Parser::new(&expr);
    match parser.parse() {
        Ok(result) => {
            let output = if result == result.trunc() && result.abs() < 1e15 {
                format!("{}", result as i64)
            } else {
                format!("{}", result)
            };

            if copy_result {
                if let Err(e) = copy_to_clipboard(&output) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }

            println!("{}", output);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_string()))
        .map_err(|e| format!("Clipboard copy failed: {}", e))
}

fn run_add_mode() -> io::Result<()> {
    let mut total = 0.0_f64;
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("#vical-add: {} > ", App::format_result(total));
        stdout.flush()?;

        let mut line = String::new();
        let read = stdin.read_line(&mut line)?;
        if read == 0 {
            break;
        }

        let input = line.trim();
        if input.eq_ignore_ascii_case("q") {
            break;
        }

        match input.parse::<f64>() {
            Ok(value) => total += value,
            Err(_) => eprintln!("Invalid number: {}", input),
        }
    }

    Ok(())
}

fn run_sub_mode() -> io::Result<()> {
    let mut total: Option<f64> = None;
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        let display = App::format_result(total.unwrap_or(0.0));
        print!("#vical-sub: {} > ", display);
        stdout.flush()?;

        let mut line = String::new();
        let read = stdin.read_line(&mut line)?;
        if read == 0 {
            break;
        }

        let input = line.trim();
        if input.eq_ignore_ascii_case("q") {
            break;
        }

        match input.parse::<f64>() {
            Ok(value) => match total {
                Some(current) => total = Some(current - value),
                None => total = Some(value),
            },
            Err(_) => eprintln!("Invalid number: {}", input),
        }
    }

    Ok(())
}

fn run_pow_mode() -> io::Result<()> {
    let mut total: Option<f64> = None;
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        let display = App::format_result(total.unwrap_or(0.0));
        print!("#vical-pow: {} > ", display);
        stdout.flush()?;

        let mut line = String::new();
        let read = stdin.read_line(&mut line)?;
        if read == 0 {
            break;
        }

        let input = line.trim();
        if input.eq_ignore_ascii_case("q") {
            break;
        }

        match input.parse::<f64>() {
            Ok(value) => match total {
                Some(current) => total = Some(current.powf(value)),
                None => total = Some(value),
            },
            Err(_) => eprintln!("Invalid number: {}", input),
        }
    }

    Ok(())
}

fn run_binary_mode(args: &[String], copy_result: bool) -> Result<(), String> {
    if args.len() != 2 {
        return Err("Usage: vical --binary <decimal_integer>".to_string());
    }

    let decimal = args[1]
        .parse::<i128>()
        .map_err(|_| format!("Invalid decimal integer: {}", args[1]))?;

    let output = format!("{:b}", decimal);
    if copy_result {
        copy_to_clipboard(&output)?;
    }
    println!("{}", output);
    Ok(())
}

fn run_from_binary_mode(args: &[String], copy_result: bool) -> Result<(), String> {
    if args.len() != 2 {
        return Err("Usage: vical --from-binary <binary_integer>".to_string());
    }

    let binary = args[1].trim();
    let decimal = i128::from_str_radix(binary, 2)
        .map_err(|_| format!("Invalid binary integer: {}", args[1]))?;

    let output = format!("{}", decimal);
    if copy_result {
        copy_to_clipboard(&output)?;
    }
    println!("{}", output);
    Ok(())
}

fn print_cli_help() {
    println!("vical - A versatile CLI calculator");
    println!();
    println!("USAGE:");
    println!("    vical [OPTIONS] [EXPRESSION]");
    println!();
    println!("COMMANDS:");
    // インタラクティブ系を一箇所にまとめ、注記を添える
    println!("    -a, --add           Interactive addition mode");
    println!("    -s, --sub           Interactive subtraction mode");
    println!("    -p, --pow           Interactive power mode");
    println!("                        (Type 'q' to return to shell)");
    println!();
    println!("    -b, --binary <DEC>  Decimal to binary conversion");
    println!("    -fb, --from-binary  Binary to decimal conversion");
    println!();
    println!("OPTIONS:");
    println!("    -c, --copy          Copy result to clipboard");
    println!("    -h, --help          Show this help message");
}

// ====== TUI ======

#[derive(PartialEq)]
enum Mode {
    Input,
    Navigate,
    Help,
}

#[derive(Clone, Copy, PartialEq)]
enum CalcMode {
    Calc,
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

impl CalcMode {
    fn label(self) -> &'static str {
        match self {
            CalcMode::Calc => "CALC",
            CalcMode::Add => "ADD",
            CalcMode::Sub => "SUB",
            CalcMode::Mul => "MUL",
            CalcMode::Div => "DIV",
            CalcMode::Pow => "POW",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            CalcMode::Calc => "",
            CalcMode::Add => "+",
            CalcMode::Sub => "-",
            CalcMode::Mul => "*",
            CalcMode::Div => "/",
            CalcMode::Pow => "^",
        }
    }
}

struct App {
    history: Vec<(String, String)>,
    input: Vec<char>,
    cursor: usize,
    selected: Option<usize>,
    mode: Mode,
    calc_mode: CalcMode,
    accumulator: Option<f64>,
    theme: Theme,
    error: Option<String>,
    yank_pending: bool,
    copied_until: Option<Instant>,
}

impl App {
    fn new(theme: Theme) -> Self {
        App {
            history: Vec::new(),
            input: Vec::new(),
            cursor: 0,
            selected: None,
            mode: Mode::Input,
            calc_mode: CalcMode::Calc,
            accumulator: None,
            theme,
            error: None,
            yank_pending: false,
            copied_until: None,
        }
    }

    fn input_string(&self) -> String {
        self.input.iter().collect()
    }

    fn is_command_input(&self) -> bool {
        self.input.first() == Some(&':')
    }

    fn is_allowed_input_char(&self, c: char) -> bool {
        if self.is_command_input() {
            c.is_ascii_lowercase()
        } else if self.input.is_empty() && self.cursor == 0 && c == ':' {
            true
        } else {
            c.is_ascii_digit() || matches!(c, '+' | '-' | '*' | '/' | '%' | '^' | 'P' | '(' | ')' | ' ')
        }
    }

    fn format_result(result: f64) -> String {
        if result == result.trunc() && result.abs() < 1e15 {
            format!("{}", result as i64)
        } else {
            format!("{}", result)
        }
    }

    fn parse_value(input: &str) -> Result<f64, String> {
        let mut parser = Parser::new(input);
        parser.parse()
    }

    fn apply_command(&mut self, command: &str) {
        let cmd = command.trim().to_ascii_lowercase();
        let next_mode = match cmd.as_str() {
            ":add" => Some(CalcMode::Add),
            ":sub" => Some(CalcMode::Sub),
            ":mul" => Some(CalcMode::Mul),
            ":div" => Some(CalcMode::Div),
            ":pow" => Some(CalcMode::Pow),
            ":calc" => Some(CalcMode::Calc),
            _ => None,
        };

        if let Some(mode) = next_mode {
            self.calc_mode = mode;
            self.accumulator = None;
            self.error = None;
        } else {
            self.error = Some(format!("Unknown command: {}", command));
        }

        self.input.clear();
        self.cursor = 0;
        self.yank_pending = false;
    }

    fn tick(&mut self) {
        if let Some(until) = self.copied_until {
            if Instant::now() >= until {
                self.copied_until = None;
            }
        }
    }

    fn copy_selected_result(&mut self) {
        let Some(sel) = self.selected else {
            return;
        };

        let history_index = self.history.len() - 1 - sel;
        let result = self.history[history_index].1.clone();

        match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(result)) {
            Ok(()) => {
                self.copied_until = Some(Instant::now() + Duration::from_secs(1));
                self.error = None;
            }
            Err(e) => {
                self.error = Some(format!("Copy failed: {}", e));
            }
        }
    }

    fn evaluate(&mut self) {
        let expr = self.input_string();
        if expr.trim().is_empty() {
            return;
        }

        if expr.trim_start().starts_with(':') {
            self.apply_command(&expr);
            return;
        }

        match Self::parse_value(&expr) {
            Ok(value) => {
                let prev_acc = self.accumulator;
                let result = match self.calc_mode {
                    CalcMode::Calc => value,
                    CalcMode::Add => self.accumulator.unwrap_or(0.0) + value,
                    CalcMode::Sub => match self.accumulator {
                        Some(acc) => acc - value,
                        None => value,
                    },
                    CalcMode::Mul => match self.accumulator {
                        Some(acc) => acc * value,
                        None => value,
                    },
                    CalcMode::Div => match self.accumulator {
                        Some(acc) => {
                            if value == 0.0 {
                                self.error = Some("Division by zero".to_string());
                                return;
                            }
                            acc / value
                        }
                        None => value,
                    },
                    CalcMode::Pow => match self.accumulator {
                        Some(acc) => acc.powf(value),
                        None => value,
                    },
                };

                if self.calc_mode != CalcMode::Calc {
                    self.accumulator = Some(result);
                }

                let history_expr = if self.calc_mode == CalcMode::Calc {
                    expr.clone()
                } else if let Some(prev) = prev_acc {
                    format!(
                        "{} {} {}",
                        Self::format_result(prev),
                        self.calc_mode.symbol(),
                        expr
                    )
                } else {
                    expr.clone()
                };

                let result_str = Self::format_result(result);
                self.history.push((history_expr, result_str));
                self.input.clear();
                self.cursor = 0;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn insert_result_at_cursor(&mut self, s: &str) {
        for (i, c) in s.chars().enumerate() {
            self.input.insert(self.cursor + i, c);
        }
        self.cursor += s.chars().count();
    }

    fn paste_clipboard_to_input(&mut self) {
        match Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
            Ok(text) => {
                for c in text.chars() {
                    if self.is_allowed_input_char(c) {
                        self.input.insert(self.cursor, c);
                        self.cursor += 1;
                    }
                }
                self.error = None;
            }
            Err(e) => {
                self.error = Some(format!("Paste failed: {}", e));
            }
        }
    }

    fn handle_key(&mut self, key: event::KeyEvent) -> bool {
        if key.code == KeyCode::Char('?') {
            self.mode = Mode::Help;
            self.selected = None;
            self.yank_pending = false;
            return false;
        }

        if self.mode == Mode::Help {
            if key.code == KeyCode::Esc {
                self.mode = Mode::Input;
            }
            self.yank_pending = false;
            return false;
        }

        if key.code == KeyCode::Char('q') && self.mode == Mode::Input && !self.is_command_input() {
            self.yank_pending = false;
            return true;
        }

        if key.code == KeyCode::Char('c') && self.mode == Mode::Input && !self.is_command_input() {
            self.accumulator = None;
            self.error = None;
            self.input.clear();
            self.cursor = 0;
            self.yank_pending = false;
            return false;
        }

        if key.code == KeyCode::Char('p') && self.mode == Mode::Input && !self.is_command_input() {
            self.paste_clipboard_to_input();
            self.yank_pending = false;
            return false;
        }

        match self.mode {
            Mode::Input => match key.code {
                KeyCode::Enter | KeyCode::Char('=') => self.evaluate(),
                KeyCode::Esc => {
                    self.input.clear();
                    self.cursor = 0;
                    self.error = None;
                    self.yank_pending = false;
                }
                KeyCode::Backspace => {
                    self.yank_pending = false;
                    if self.error.is_some() {
                        self.error = None;
                    } else if self.cursor > 0 {
                        self.cursor -= 1;
                        self.input.remove(self.cursor);
                    }
                }
                KeyCode::Delete => {
                    self.yank_pending = false;
                    if self.cursor < self.input.len() {
                        self.input.remove(self.cursor);
                        self.error = None;
                    }
                }
                KeyCode::Left => {
                    self.yank_pending = false;
                    if self.cursor > 0 {
                        self.cursor -= 1;
                    }
                }
                KeyCode::Right => {
                    self.yank_pending = false;
                    if self.cursor < self.input.len() {
                        self.cursor += 1;
                    }
                }
                KeyCode::Home => {
                    self.yank_pending = false;
                    self.cursor = 0;
                }
                KeyCode::End => {
                    self.yank_pending = false;
                    self.cursor = self.input.len();
                }
                KeyCode::Char('j') | KeyCode::Down if !self.history.is_empty() => {
                    self.mode = Mode::Navigate;
                    self.selected = Some(0);
                    self.yank_pending = false;
                }
                KeyCode::Char('k') | KeyCode::Up if !self.history.is_empty() => {
                    self.mode = Mode::Navigate;
                    self.selected = Some(0);
                    self.yank_pending = false;
                }
                KeyCode::Char(c) if self.is_allowed_input_char(c) => {
                    self.yank_pending = false;
                    self.input.insert(self.cursor, c);
                    self.cursor += 1;
                    self.error = None;
                }
                _ => {
                    self.yank_pending = false;
                }
            },

            Mode::Navigate => match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Input;
                    self.selected = None;
                    self.yank_pending = false;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.yank_pending = false;
                    if let Some(sel) = self.selected {
                        if sel > 0 {
                            self.selected = Some(sel - 1);
                        }
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.yank_pending = false;
                    if let Some(sel) = self.selected {
                        if sel + 1 < self.history.len() {
                            self.selected = Some(sel + 1);
                        }
                    }
                }
                KeyCode::Enter => {
                    self.yank_pending = false;
                    if let Some(sel) = self.selected {
                        let history_index = self.history.len() - 1 - sel;
                        let result = self.history[history_index].1.clone();
                        self.insert_result_at_cursor(&result);
                        self.mode = Mode::Input;
                        self.selected = None;
                    }
                }
                KeyCode::Char('y') => {
                    if self.yank_pending {
                        self.yank_pending = false;
                        self.copy_selected_result();
                    } else {
                        self.yank_pending = true;
                    }
                }
                KeyCode::Char(c) if self.is_allowed_input_char(c) => {
                    self.yank_pending = false;
                    self.mode = Mode::Input;
                    self.selected = None;
                    self.input.insert(self.cursor, c);
                    self.cursor += 1;
                    self.error = None;
                }
                KeyCode::Backspace => {
                    self.yank_pending = false;
                    self.mode = Mode::Input;
                    self.selected = None;
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        self.input.remove(self.cursor);
                        self.error = None;
                    }
                }
                _ => {
                    self.yank_pending = false;
                    self.mode = Mode::Input;
                    self.selected = None;
                }
            },

            Mode::Help => {}
        }
        false
    }
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(area);

    let items: Vec<ListItem> = app
        .history
        .iter()
        .rev()
        .map(|(expr, result)| ListItem::new(format!("{} = {}", expr, result)))
        .collect();

    let mut list_state = ListState::default();
    list_state.select(app.selected);

    if app.mode == Mode::Help {
        let help_lines = [
            "",
            "General",
            "  q: Quit app (Input mode)",
            "  ?: Open help",
            "  p: Paste clipboard into input",
            "  Esc: Close help / cancel history selection",
            "",
            "History",
            "  j or Down: Move to newer entry",
            "  k or Up: Move to older entry",
            "  Enter: Insert selected result",
            "  yy: Copy selected result",
            "",
            "Commands",
            "  :add  :sub  :mul  :div  :pow  :calc",
            "",
            "Sequence",
            "  c: Clear sequence accumulator",
        ]
        .join("\n");

        let help = Paragraph::new(help_lines).block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(help, chunks[0]);
    } else {
        let selected_color = app.theme.accent_bg;
        let history_block = if app.mode == Mode::Navigate {
            Block::default()
                .borders(Borders::ALL)
                .title("History")
                .border_style(Style::default().fg(selected_color))
                .title_style(Style::default().fg(selected_color))
        } else {
            Block::default().borders(Borders::ALL).title("History")
        };

        let list = List::new(items)
            .direction(ListDirection::BottomToTop)
            .block(history_block)
            .highlight_style(
                Style::default()
                    .bg(selected_color)
                    .fg(app.theme.accent_fg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, chunks[0], &mut list_state);
    }

    // Split bottom area into 2 rows: status line + input line
    let bottom_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(chunks[1]);

    // Status line (mode + help) - gray background, black text
    let mode_text = match app.mode {
        Mode::Input => app.calc_mode.label().to_string(),
        Mode::Navigate => format!("SELECT/{}", app.calc_mode.label()),
        Mode::Help => "HELP".to_string(),
    };
    let help_text = match app.mode {
        Mode::Input => "[Enter:run  q:quit  c:clear-seq  p:paste  j/k/↑/↓:history  :add/:sub/:mul/:div/:pow/:calc]",
        Mode::Navigate => "[j/↓:newer  k/↑:older  Enter:insert  Esc:cancel]",
        Mode::Help => "[Esc:back]",
    };
    let status_line = format!("{:<8} {}", mode_text, help_text);
    let status_bg = if app.mode == Mode::Input {
        app.theme.accent_bg
    } else {
        Color::Gray
    };
    let status_fg = if app.mode == Mode::Input {
        app.theme.accent_fg
    } else {
        Color::Black
    };
    let status = Paragraph::new(status_line)
        .style(Style::default().bg(status_bg).fg(status_fg));
    f.render_widget(status, bottom_chunks[0]);

    // Input line with "#vical > " prefix
    let prefix = "#vical > ";
    let (input_text, input_style) = if app.copied_until.is_some() {
        (
            format!("{}copied!", prefix),
            Style::default()
                .fg(app.theme.accent_bg)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
    } else if let Some(err) = &app.error {
        (format!("{}{}", prefix, err), Style::default().fg(Color::Red))
    } else {
        (format!("{}{}", prefix, app.input_string()), Style::default())
    };

    let input_para = Paragraph::new(input_text).style(input_style);
    f.render_widget(input_para, bottom_chunks[1]);

    // Cursor positioning
    if app.mode == Mode::Input && app.error.is_none() {
        let prefix_len = prefix.len() as u16;
        let cursor_x = bottom_chunks[1].x + prefix_len + app.cursor as u16;
        let cursor_y = bottom_chunks[1].y;
        let max_x = bottom_chunks[1].x + bottom_chunks[1].width;
        if cursor_x <= max_x {
            f.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn run_tui() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let theme = load_theme();
    let result = run_app(&mut terminal, theme);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, theme: Theme) -> io::Result<()> {
    let mut app = App::new(theme);
    loop {
        app.tick();
        terminal.draw(|f| ui(f, &app))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }

        if let Event::Key(key) = event::read()? {
            if key.kind != event::KeyEventKind::Press {
                continue;
            }
            if app.handle_key(key) {
                break;
            }
        }
    }
    Ok(())
}

// ====== 数式パーサー ======

struct Parser<'a> {
    chars: Peekable<Chars<'a>>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser {
            chars: input.chars().peekable(),
        }
    }

    fn parse(&mut self) -> Result<f64, String> {
        let result = self.expr()?;
        self.skip_whitespace();
        if self.chars.peek().is_some() {
            Err(format!("Unexpected character: '{}'", self.chars.peek().unwrap()))
        } else {
            Ok(result)
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.chars.peek(), Some(&' ') | Some(&'\t')) {
            self.chars.next();
        }
    }

    fn expr(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        let mut left = self.term()?;
        loop {
            self.skip_whitespace();
            match self.chars.peek() {
                Some(&'+') => {
                    self.chars.next();
                    left += self.term()?;
                }
                Some(&'-') => {
                    self.chars.next();
                    left -= self.term()?;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        let mut left = self.power()?;
        loop {
            self.skip_whitespace();
            match self.chars.peek() {
                Some(&'*') => {
                    self.chars.next();
                    left *= self.power()?;
                }
                Some(&'/') => {
                    self.chars.next();
                    let right = self.power()?;
                    if right == 0.0 {
                        return Err("Division by zero".to_string());
                    }
                    left /= right;
                }
                Some(&'%') => {
                    self.chars.next();
                    let right = self.power()?;
                    if right == 0.0 {
                        return Err("Division by zero".to_string());
                    }
                    left %= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn power(&mut self) -> Result<f64, String> {
        let base = self.factor()?;
        self.skip_whitespace();
        if self.chars.peek() == Some(&'^') {
            self.chars.next();
            let exp = self.power()?; // right-associative
            Ok(base.powf(exp))
        } else {
            Ok(base)
        }
    }

    fn factor(&mut self) -> Result<f64, String> {
        self.skip_whitespace();
        match self.chars.peek() {
            Some(&'-') => {
                self.chars.next();
                Ok(-self.factor()?)
            }
            Some(&'+') => {
                self.chars.next();
                self.factor()
            }
            Some(&'(') => {
                self.chars.next();
                let val = self.expr()?;
                self.skip_whitespace();
                if self.chars.next() != Some(')') {
                    return Err("')' not found".to_string());
                }
                Ok(val)
            }
            Some(&c) if c.is_ascii_digit() || c == '.' => self.number(),
            Some(&c) => Err(format!("Unexpected character: '{}'", c)),
            None => Err("Expression ended unexpectedly".to_string()),
        }
    }

    fn number(&mut self) -> Result<f64, String> {
        let mut s = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_digit() || c == '.' {
                s.push(c);
                self.chars.next();
            } else {
                break;
            }
        }
        s.parse::<f64>().map_err(|_| format!("Invalid number: '{}'", s))
    }
}
