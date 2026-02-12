mod types;
mod utils;

use ansi_to_tui::IntoText as _;
use anyhow::{bail, Context};
use clap::Parser;
use chrono::NaiveDateTime;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};
// use itertools::Itertools;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Text, Line, Span},
    widgets::{
        Block, Borders, BorderType, List, ListItem, ListState,
        Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    error::Error, fs, io,
    path::{Path, PathBuf},
};

macro_rules! style_ {
    ($s:expr, bg $w:ident) => { $s.bg(Color::$w) };
    ($s:expr, fg $w:ident) => { $s.fg(Color::$w) };
    ($s:expr, mo $w:ident) => { $s.add_modifier(Modifier::$w) };
}

macro_rules! style {
    ($($k:ident $i:ident),+$(,)*) => {{
        let mut s = Style::default();
        $(s = style_!(s, $k $i);)+
        s
    }};
}

macro_rules! span {
    ($s:expr $(,$k:ident $i:ident)*) => {{
        let st = {
            #[allow(unused_mut)]
            let mut s = Style::default();
            $(s = style_!(s, $k $i);)*
            s
        };
        Span::styled(std::borrow::Cow::from($s), st)
    }};
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(short = 'C', long, default_value = ".")]
    dir: String,

    #[arg(short = 'n', long)]
    file_name: String,

    #[arg(short = 'd', long)]
    late_deadline: String,

    #[arg(short = 'D', long)]
    final_deadline: String,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
enum Mode {
    #[default]
    Overview,
    Readme,
    Program,
    Tests,
    CompileLog,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Report {
    #[serde(default)] compiled: bool,
    #[serde(default)] passed_tests: bool,
    #[serde(default)] readme_approved: Option<bool>,
    #[serde(default)] readme_penalty: Option<f32>,
    #[serde(default)] code_approved: Option<bool>,
    #[serde(default)] tests_score_total: f32,
    #[serde(default)] tests_score_override: Option<f32>,
    #[serde(default)] manual_deductions: Vec<(f32, String)>,
    date: String,
}

impl Report {
    pub fn tests_score(&self) -> f32 {
        self.tests_score_override.unwrap_or(self.tests_score_total)
    }
}

struct Submission {
    student: String,
    path: PathBuf,

    readme: String,
    program: String,
    test_log: String,
    compile_log: String,

    comments: String,
    report: Report,
}

struct App {
    opts: Args,
    list_state: ListState,
    submissions: Vec<Submission>,

    scroll_offset: usize,
    mode: Mode,
}

impl App {
    fn new(args: Args) -> anyhow::Result<Self> {
        let mut submissions = Vec::new();

        // Read all subdirectories
        let dir = Path::new(&args.dir);
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    // Load README.txt
                    let readme_path = path.join("README.txt");
                    let readme = match fs::read_to_string(&readme_path) {
                        Ok(s) => s,
                        Err(e) => format!("{e:?}"),
                    };

                    // Load comments
                    let comments_path = path.join("comments.txt");
                    let comments = fs::read_to_string(&comments_path)
                        .unwrap_or(String::new());

                    // Load program code
                    let program_path = path.join(&args.file_name);
                    let program = fs::read_to_string(&program_path)
                        .unwrap_or(format!("Could not read file {}", program_path.display()));

                    // Load test log
                    let log_path = path.join("test_log.txt");
                    let test_log = fs::read_to_string(&log_path)
                        .unwrap_or(format!("Could not read file {}", log_path.display()));

                    // Load test log
                    let comp_log_path = path.join("compile_log.txt");
                    let compile_log = fs::read_to_string(&comp_log_path)
                        .unwrap_or(format!("Could not read file {}", comp_log_path.display()));

                    // Load status.json
                    let report_path = path.join("status.json");
                    let report_str = fs::read_to_string(&report_path)
                        .context("Failed to read status.json")?;
                    let report = serde_json::from_str(&report_str)
                        .context(format!("Failed to parse {}", report_path.display()))?;

                    submissions.push(Submission {
                        student: path.file_name().unwrap()
                            .to_string_lossy()
                            .trim_end_matches(".stud")
                            .to_string(),
                        path, readme, comments, program, test_log, compile_log, report,
                    });
                }
            }
        } else {
            bail!("Provided directory is not a directory");
        }

        submissions.sort_by_key(|s| s.student.clone());

        let mut app = App {
            submissions,
            opts: args,
            list_state: ListState::default(),
            scroll_offset: 0,
            mode: Mode::default(),
        };

        assert!(!app.submissions.is_empty());
        app.list_state.select(Some(0));

        Ok(app)
    }

    fn save_report(&self) {
        let submission = self.submission();
        let report_path = submission.path.join("status.json");
        if let Ok(json) = serde_json::to_string_pretty(&submission.report) {
            let _ = fs::write(&report_path, json);
        }
        let comments_path = submission.path.join("comments.txt");
        let _ = fs::write(&comments_path, &submission.comments);
    }

    fn next_directory(&mut self) -> anyhow::Result<()> {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.submissions.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.scroll_offset = 0;
        Ok(())
    }

    fn previous_directory(&mut self) -> anyhow::Result<()> {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.submissions.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.scroll_offset = 0;
        Ok(())
    }

    fn next_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Overview => Mode::Readme,
            Mode::Readme => Mode::Tests,
            Mode::Tests => Mode::CompileLog,
            Mode::CompileLog => Mode::Program,
            Mode::Program => Mode::Overview,
        };
        self.scroll_offset = 0;
    }

    fn previous_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Overview => Mode::Program,
            Mode::Readme => Mode::Overview,
            Mode::Tests => Mode::Readme,
            Mode::CompileLog => Mode::Tests,
            Mode::Program => Mode::CompileLog,
        };
        self.scroll_offset = 0;
    }

    fn approval(&self) -> Option<bool> {
        match self.mode {
            Mode::Readme => self.submission().report.readme_approved,
            Mode::Program => self.submission().report.code_approved,
            _ => unreachable!(),
        }
    }

    fn toggle_approval(&mut self) {
        match self.mode {
            Mode::Readme => self.submission_mut().report.readme_approved = Some(!self.submission().report.readme_approved.unwrap_or(false)),
            Mode::Program => self.submission_mut().report.code_approved = Some(!self.submission().report.code_approved.unwrap_or(false)),
            _ => (),
        }
        self.save_report();
    }

    fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn submission(&self) -> &Submission {
        &self.submissions[self.list_state.selected().unwrap()]
    }

    fn submission_mut(&mut self) -> &mut Submission {
        &mut self.submissions[self.list_state.selected().unwrap()]
    }

    fn content(&self) -> &str {
        match self.mode {
            Mode::Overview => "",
            Mode::Readme => &self.submission().readme,
            Mode::Program => &self.submission().program,
            Mode::Tests => &self.submission().test_log,
            Mode::CompileLog => &self.submission().compile_log,
        }
    }

    fn scroll_down(&mut self, max_lines: usize) {
        let content_lines = self.content().lines().count();
        if self.scroll_offset + max_lines < content_lines {
            self.scroll_offset += 1;
        }
    }
}

fn edit<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, s: &str) -> String {
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture).unwrap();
    disable_raw_mode().unwrap();
    let s = edit::edit(&s).unwrap();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
    enable_raw_mode().unwrap();
    terminal.clear().unwrap();
    s
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(args)?;

    // Main loop
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app)).unwrap();

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('c') => {
                        app.submission_mut().comments = edit(terminal, &app.submission().comments);
                        app.save_report();
                    },
                    KeyCode::Char('v') => {
                        _ = edit(terminal, match app.mode {
                            Mode::Overview => continue,
                            Mode::Program => &app.submission().program,
                            Mode::Readme => &app.submission().readme,
                            Mode::Tests => &app.submission().test_log,
                            Mode::CompileLog => &app.submission().compile_log,
                        });
                    },
                    KeyCode::Down | KeyCode::Char('j') => app.next_directory()?,
                    KeyCode::Up | KeyCode::Char('k') => app.previous_directory()?,
                    KeyCode::Left | KeyCode::Char('h') => app.previous_mode(),
                    KeyCode::Right | KeyCode::Char('l') => app.next_mode(),
                    KeyCode::Char(' ') => app.toggle_approval(),
                    KeyCode::PageDown => {
                        for _ in 0..10 {
                            app.scroll_down(20);
                        }
                    }
                    KeyCode::PageUp => {
                        for _ in 0..10 {
                            app.scroll_up();
                        }
                    }
                    KeyCode::Char('d') => {
                        for _ in 0..5 {
                            app.scroll_down(20);
                        }
                    }
                    KeyCode::Char('u') => {
                        for _ in 0..5 {
                            app.scroll_up();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let submissions = app.submissions
        .iter()
        .map(|sub| (
            types::score(sub, &app.opts.late_deadline, &app.opts.final_deadline).unwrap(),
            sub.student.clone()
        ))
        .collect::<Vec<_>>();
    let average = submissions.iter().map(|(s, _)| s.total).sum::<f32>() / submissions.len() as f32;

    let items: Vec<ListItem> = submissions.clone()
        .into_iter()
        .map(|(score, name)|
            ListItem::new(Line::from(vec![
                span!(format!(" {: >4}  ", score.total), fg Green),
                span!(name)
            ]))
        )
        .collect();
    let max_item_width = items.iter().map(|i| i.width()).max().unwrap_or(5) as u16 + 3;

    // Create the layout
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(max_item_width), Constraint::Fill(5), Constraint::Fill(2)])
        .spacing(1)
        .split(f.area());

    // Render sidebar
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_type(BorderType::QuadrantInside)
        .padding(Padding::right(1));

    let sidebar_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(items.len() as u16), Constraint::Min(1)])
        .spacing(1)
        .split(block.inner(chunks[0]));

    let items = List::new(items)
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::Black));

    f.render_stateful_widget(items, sidebar_chunks[0], &mut app.list_state);

    let widg = Paragraph::new(Line::from(vec![
            span!("Average: ", mo BOLD),
            Span::styled(
                ((average * 10.).round() / 10.).to_string(),
                utils::style_for_score(average),
            ),
        ]));

    f.render_widget(widg, sidebar_chunks[1]);

    f.render_widget(block, chunks[0]);

    render_main_view(f, app, chunks[1]);
    render_comments(f, app, chunks[2]);
}

fn render_main_view(f: &mut Frame, app: &mut App, area: Rect) {
    // Split main area into approval toggle and content
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Render mode toggle
    let modes = &[Mode::Overview, Mode::Readme, Mode::Tests, Mode::CompileLog, Mode::Program];
    let line = Line::from_iter(
        modes.iter().flat_map(|&mode| {
            let style =
                if mode == app.mode {
                    style!(fg Black, bg Blue, mo BOLD)
                } else {
                    Style::default()
                };
            vec![
                Span::styled(format!(" {:?} ", mode), style),
                Span::raw(" ")
            ]
        })
    );

    let mode_widget = Paragraph::new(line)
        .block(Block::default().borders(Borders::TOP).title("Mode"));
    f.render_widget(mode_widget, main_chunks[0]);

    match app.mode {
        Mode::Overview => (),
        Mode::Tests => {
            let score = app.submission().report.tests_score();
            let widg = Paragraph::new(Line::from(vec![
                    span!("Score: ", mo BOLD),
                    Span::styled(score.to_string(), utils::style_for_score(score)),
                ]))
                .block(Block::default().borders(Borders::TOP).title("Status"));

            f.render_widget(widg, main_chunks[1]);
        },
        Mode::CompileLog => {},
        _ => render_approval(f, app.approval(), main_chunks[1]),
    }

    match app.mode {
        Mode::Overview => render_overview(f, app, main_chunks[2]),
        _ => render_content(f, app.content(), app.scroll_offset, main_chunks[2]),
    }
}

fn render_approval(f: &mut Frame, approved: Option<bool>, area: Rect) {
    let approval_text = match approved {
        Some(true) => "✓ Approved",
        Some(false) => "✗ Not Approved",
        None => "? Unchecked",
    };

    let approval_style = match approved {
        Some(true) => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        Some(false) => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        None => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    };

    let approval_widget = Paragraph::new(approval_text)
        .style(approval_style)
        .block(Block::default().borders(Borders::TOP).title("Status"));

    f.render_widget(approval_widget, area);
}

fn render_overview(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    let mut l = |a, b| lines.push(Line::from(vec![a, b]));

    let score = types::score(app.submission(), &app.opts.late_deadline, &app.opts.final_deadline).unwrap();

    l(span!("Submission date: ", mo BOLD), span!(score.submitted.to_string(), fg Blue));
    l(span!(""), span!(""));

    let base_score_label = if app.submission().report.tests_score_override.is_some() { "Base score (override): " } else { "Base score: " };
    l(span!(base_score_label, mo BOLD), span!(score.base.to_string(), fg Blue));

    use types::ScoreItem::*;
    for item in score.items {
        match item {
            Comment(s) => l(span!("    ..  "), span!(s, mo ITALIC)),
            Alert(s) => l(span!("    !!  ", fg Red), span!(s, mo ITALIC)),
            Deduction(v, s) => l(span!(format!("{: >6}  ", format!("-{v}")), fg Red), span!(s, mo ITALIC)),
        }
    }
    l(span!("Total: ", mo BOLD), span!(score.total.to_string(), fg Blue));

    let paragraph = Paragraph::new(lines)
        .wrap(Default::default())
        .block(Block::default().borders(Borders::TOP).title("Content"));

    f.render_widget(paragraph, area);
}

fn render_content(f: &mut Frame, content: &str, scroll_offset: usize, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize; // -2 for borders

    let mut content = content.into_text().unwrap();
    let mut lineno = 1;
    for line in &mut content.lines {
        line.spans.insert(0, span!(format!("{lineno: >4}  "), fg Magenta, mo ITALIC));
        lineno += 1;
    }
    let lines: Vec<Line> = content.lines.clone().into_iter().skip(scroll_offset).take(inner_height).collect();

    let paragraph = Paragraph::new(lines)
        .wrap(Default::default())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Content")
                .padding(Padding::new(1, 1, 1, 1))
        );

    f.render_widget(paragraph, area);

    // Render scrollbar
    let total_lines = content.lines.len();
    if total_lines > inner_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state = ScrollbarState::new(total_lines.saturating_sub(inner_height))
            .position(scroll_offset);

        f.render_stateful_widget(
            scrollbar,
            area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn render_comments(f: &mut Frame, app: &mut App, area: Rect) {
    let mode_widget = Paragraph::new(Text::from(app.submission().comments.as_str()))
        .wrap(Default::default())
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_type(BorderType::QuadrantInside)
                .padding(Padding::left(1))
        );
    f.render_widget(mode_widget, area);
}

// Parse eLC's utterly demented date format. Written be Claude because I can't be bothered to.
fn parse_stupid_date(date_str: &str) -> anyhow::Result<NaiveDateTime> {
    // The format "Feb 6, 2026 414 PM" - we can't preprocess it
    // So we need to manually parse each component

    // Split the string into parts
    let parts: Vec<&str> = date_str.split_whitespace().collect();

    if parts.len() < 4 {
        bail!("Invalid date format");
    }

    // parts[0] = "Feb"
    // parts[1] = "6,"  (with comma)
    // parts[2] = "2026"
    // parts[3] = "414"
    // parts[4] = "PM"

    let month = parts[0];
    let day = parts[1].trim_end_matches(',');
    let year = parts[2];
    let time_digits = parts[3]; // "414"
    let am_pm = parts[4]; // "PM"

    // Parse the time digits: "414" means 4:14
    // For 3 digits: first digit is hour, last two are minutes
    // For 4 digits: first two are hour, last two are minutes
    let (hour, minute) = if time_digits.len() == 3 {
        // "414" -> hour=4, minute=14
        let h = time_digits[0..1].parse::<u32>()?;
        let m = time_digits[1..3].parse::<u32>()?;
        (h, m)
    } else if time_digits.len() == 4 {
        // "1214" -> hour=12, minute=14
        let h = time_digits[0..2].parse::<u32>()?;
        let m = time_digits[2..4].parse::<u32>()?;
        (h, m)
    } else {
        bail!("Invalid date format");
    };

    // Convert to 24-hour format
    let hour_24 = if am_pm == "PM" && hour != 12 {
        hour + 12
    } else if am_pm == "AM" && hour == 12 {
        0
    } else {
        hour
    };

    // Construct a string that chrono can parse
    let formatted = format!("{} {}, {} {:02}:{:02}:00", month, day, year, hour_24, minute);

    // Parse with format: "Feb 6, 2026 16:14:00"
    Ok(NaiveDateTime::parse_from_str(&formatted, "%b %d, %Y %H:%M:%S")?)
}
