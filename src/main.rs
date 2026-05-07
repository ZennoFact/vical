use std::env;
use std::io;
use std::iter::Peekable;
use std::str::Chars;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
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

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 2 && args[1] == "-v" {
        if let Err(e) = run_tui() {
            eprintln!("TUIエラー: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if args.len() < 2 {
        eprintln!("使い方: vical <式>");
        eprintln!("       vical -v  (対話モード)");
        std::process::exit(1);
    }

    let expr: String = args[1..].concat();
    let mut parser = Parser::new(&expr);
    match parser.parse() {
        Ok(result) => {
            if result == result.trunc() && result.abs() < 1e15 {
                println!("{}", result as i64);
            } else {
                println!("{}", result);
            }
        }
        Err(e) => {
            eprintln!("エラー: {}", e);
            std::process::exit(1);
        }
    }
}

// ====== TUI ======

#[derive(PartialEq)]
enum Mode {
    Input,
    Navigate,
}

struct App {
    history: Vec<(String, String)>,
    input: Vec<char>,
    cursor: usize,
    selected: Option<usize>,
    mode: Mode,
    error: Option<String>,
}

impl App {
    fn new() -> Self {
        App {
            history: Vec::new(),
            input: Vec::new(),
            cursor: 0,
            selected: None,
            mode: Mode::Input,
            error: None,
        }
    }

    fn input_string(&self) -> String {
        self.input.iter().collect()
    }

    fn format_result(result: f64) -> String {
        if result == result.trunc() && result.abs() < 1e15 {
            format!("{}", result as i64)
        } else {
            format!("{}", result)
        }
    }

    fn evaluate(&mut self) {
        let expr = self.input_string();
        if expr.trim().is_empty() {
            return;
        }
        let mut parser = Parser::new(&expr);
        match parser.parse() {
            Ok(result) => {
                let result_str = Self::format_result(result);
                self.history.push((expr, result_str));
                self.input.clear();
                self.cursor = 0;
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e);
            }
        }
    }

    fn insert_result_at_cursor(&mut self, s: &str) {
        for (i, c) in s.chars().enumerate() {
            self.input.insert(self.cursor + i, c);
        }
        self.cursor += s.chars().count();
    }

    fn handle_key(&mut self, key: event::KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return true;
        }

        match self.mode {
            Mode::Input => match key.code {
                KeyCode::Enter => self.evaluate(),
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        self.input.remove(self.cursor);
                        self.error = None;
                    }
                }
                KeyCode::Delete => {
                    if self.cursor < self.input.len() {
                        self.input.remove(self.cursor);
                        self.error = None;
                    }
                }
                KeyCode::Left => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                    }
                }
                KeyCode::Right => {
                    if self.cursor < self.input.len() {
                        self.cursor += 1;
                    }
                }
                KeyCode::Home => self.cursor = 0,
                KeyCode::End => self.cursor = self.input.len(),
                KeyCode::Char('j') if !self.history.is_empty() => {
                    self.mode = Mode::Navigate;
                    self.selected = Some(0);
                }
                KeyCode::Char('k') if !self.history.is_empty() => {
                    self.mode = Mode::Navigate;
                    self.selected = Some(0);
                }
                KeyCode::Char(c) => {
                    self.input.insert(self.cursor, c);
                    self.cursor += 1;
                    self.error = None;
                }
                _ => {}
            },

            Mode::Navigate => match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Input;
                    self.selected = None;
                }
                KeyCode::Char('j') => {
                    if let Some(sel) = self.selected {
                        if sel > 0 {
                            self.selected = Some(sel - 1);
                        }
                    }
                }
                KeyCode::Char('k') => {
                    if let Some(sel) = self.selected {
                        if sel + 1 < self.history.len() {
                            self.selected = Some(sel + 1);
                        }
                    }
                }
                KeyCode::Enter => {
                    if let Some(sel) = self.selected {
                        let history_index = self.history.len() - 1 - sel;
                        let result = self.history[history_index].1.clone();
                        self.insert_result_at_cursor(&result);
                        self.mode = Mode::Input;
                        self.selected = None;
                    }
                }
                KeyCode::Char(c) => {
                    self.mode = Mode::Input;
                    self.selected = None;
                    self.input.insert(self.cursor, c);
                    self.cursor += 1;
                    self.error = None;
                }
                KeyCode::Backspace => {
                    self.mode = Mode::Input;
                    self.selected = None;
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        self.input.remove(self.cursor);
                        self.error = None;
                    }
                }
                _ => {
                    self.mode = Mode::Input;
                    self.selected = None;
                }
            },
        }
        false
    }
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    let items: Vec<ListItem> = app
        .history
        .iter()
        .rev()
        .map(|(expr, result)| ListItem::new(format!("{} = {}", expr, result)))
        .collect();

    let mut list_state = ListState::default();
    list_state.select(app.selected);

    let list_title = match app.mode {
        Mode::Navigate => "履歴  [j:↓新  k:↑古  Enter:挿入  Esc:戻る]",
        Mode::Input => "履歴  [j/k で選択]",
    };

    let list = List::new(items)
        .direction(ListDirection::BottomToTop)
        .block(Block::default().borders(Borders::ALL).title(list_title))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, chunks[0], &mut list_state);

    let (input_display, border_style) = if let Some(err) = &app.error {
        (
            format!("エラー: {}", err),
            Style::default().fg(Color::Red),
        )
    } else {
        (app.input_string(), Style::default())
    };

    let input_title = match app.mode {
        Mode::Input => "計算式  [Enter:計算  Ctrl+C:終了]",
        Mode::Navigate => "計算式  [選択モード]",
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(input_title)
        .border_style(border_style);

    f.render_widget(Paragraph::new(input_display).block(input_block), chunks[1]);

    if app.mode == Mode::Input && app.error.is_none() {
        let cursor_x = chunks[1].x + 1 + app.cursor as u16;
        let cursor_y = chunks[1].y + 1;
        let max_x = chunks[1].x + chunks[1].width.saturating_sub(2);
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

    let result = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();
    loop {
        terminal.draw(|f| ui(f, &app))?;
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
            Err(format!("予期しない文字: '{}'", self.chars.peek().unwrap()))
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
        let mut left = self.factor()?;
        loop {
            self.skip_whitespace();
            match self.chars.peek() {
                Some(&'*') => {
                    self.chars.next();
                    left *= self.factor()?;
                }
                Some(&'/') => {
                    self.chars.next();
                    let right = self.factor()?;
                    if right == 0.0 {
                        return Err("ゼロ除算".to_string());
                    }
                    left /= right;
                }
                Some(&'%') => {
                    self.chars.next();
                    let right = self.factor()?;
                    if right == 0.0 {
                        return Err("ゼロ除算".to_string());
                    }
                    left %= right;
                }
                _ => break,
            }
        }
        Ok(left)
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
                    return Err("')' が見つかりません".to_string());
                }
                Ok(val)
            }
            Some(&c) if c.is_ascii_digit() || c == '.' => self.number(),
            Some(&c) => Err(format!("予期しない文字: '{}'", c)),
            None => Err("式が途中で終わっています".to_string()),
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
        s.parse::<f64>().map_err(|_| format!("無効な数値: '{}'", s))
    }
}
