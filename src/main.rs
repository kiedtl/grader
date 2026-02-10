use clap::Parser;
use anyhow::{bail, Context};
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

#[derive(Parser, Debug)]
struct Args {
    #[arg(short = 'd', long, default_value = ".")]
    dir: String,

    #[arg(short = 'n', long)]
    file_name: String,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
enum Mode {
    #[default]
    Readme,
    Program,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Report {
    #[serde(default)] compiled: bool,
    #[serde(default)] passed_tests: bool,
    #[serde(default)] readme_approved: Option<bool>,
    #[serde(default)] code_approved: Option<bool>,
    #[serde(default)] tests_score_total: f32,
}

struct App {
    opts: Args,
    directories: Vec<PathBuf>,
    list_state: ListState,
    readme_content: String,
    program_content: String,
    comments: String,
    scroll_offset: usize,
    mode: Mode,
    report: Report,
}

impl App {
    fn new(args: Args) -> anyhow::Result<Self> {
        let mut directories = Vec::new();

        // Read all subdirectories
        let dir = Path::new(&args.dir);
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    // Check if it has both README.txt and status.json
                    let readme = path.join("README.txt");
                    let report = path.join("status.json");
                    if readme.exists() && report.exists() {
                        directories.push(path);
                    }
                }
            }
        } else {
            bail!("Provided directory is not a directory");
        }

        directories.sort();

        let mut app = App {
            directories,
            opts: args,
            list_state: ListState::default(),
            readme_content: String::new(),
            program_content: String::new(),
            comments: String::new(),
            scroll_offset: 0,
            mode: Mode::default(),
            report: Report::default(),
        };

        // Select the first directory by default
        if !app.directories.is_empty() {
            app.list_state.select(Some(0));
            app.load_current_directory()?;
        }

        Ok(app)
    }

    fn load_current_directory(&mut self) -> anyhow::Result<()> {
        if let Some(selected) = self.list_state.selected() {
            if let Some(dir) = self.directories.get(selected) {
                // Load README.txt
                let readme_path = dir.join("README.txt");
                self.readme_content = match fs::read_to_string(&readme_path) {
                    Ok(s) => s,
                    Err(e) => format!("{e:?}"),
                };

                // Load comments
                let comments_path = dir.join("comments.txt");
                self.comments = fs::read_to_string(&comments_path)
                    .unwrap_or(String::new());

                // Load program code
                let program_path = dir.join(&self.opts.file_name);
                self.program_content = fs::read_to_string(&program_path)
                    .unwrap_or(format!("Could not read file {}", program_path.display()));

                // Load status.json
                let report_path = dir.join("status.json");
                let report_str = fs::read_to_string(&report_path)
                    .context("Failed to read status.json")?;
                self.report = serde_json::from_str(&report_str)
                    .context("Failed to parse status.json")?;

                self.scroll_offset = 0;
            }
        }
        Ok(())
    }

    fn save_report(&self) {
        if let Some(selected) = self.list_state.selected() {
            if let Some(dir) = self.directories.get(selected) {
                let report_path = dir.join("status.json");
                if let Ok(json) = serde_json::to_string_pretty(&self.report) {
                    let _ = fs::write(&report_path, json);
                }
                let comments_path = dir.join("comments.txt");
                let _ = fs::write(&comments_path, &self.comments);
            }
        }
    }

    fn next_directory(&mut self) -> anyhow::Result<()> {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.directories.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.load_current_directory()?;
        Ok(())
    }

    fn previous_directory(&mut self) -> anyhow::Result<()> {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.directories.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.load_current_directory()?;
        Ok(())
    }

    fn next_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Program => Mode::Readme,
            Mode::Readme => Mode::Program,
        };
    }

    fn previous_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Program => Mode::Readme,
            Mode::Readme => Mode::Program,
        };
    }

    fn approval(&self) -> Option<bool> {
        match self.mode {
            Mode::Readme => self.report.readme_approved,
            Mode::Program => self.report.code_approved,
        }
    }

    fn toggle_approval(&mut self) {
        match self.mode {
            Mode::Readme => self.report.readme_approved = Some(!self.report.readme_approved.unwrap_or(false)),
            Mode::Program => self.report.code_approved = Some(!self.report.code_approved.unwrap_or(false)),
        }
        self.save_report();
    }

    fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn content(&self) -> &str {
        match self.mode {
            Mode::Readme => &self.readme_content,
            Mode::Program => &self.program_content,
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
                        app.comments = edit(terminal, &app.comments);
                        app.save_report();
                    },
                    KeyCode::Char('v') => {
                        _ = edit(terminal, match app.mode {
                            Mode::Program => &app.program_content,
                            Mode::Readme => &app.readme_content,
                        });
                    },
                    KeyCode::Down | KeyCode::Char('j') => app.next_directory()?,
                    KeyCode::Up | KeyCode::Char('k') => app.previous_directory()?,
                    KeyCode::Left | KeyCode::Char('h') => app.next_mode(),
                    KeyCode::Right | KeyCode::Char('l') => app.previous_mode(),
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
    let items: Vec<ListItem> = app
        .directories
        .iter()
        .map(|dir| {
            let name = dir.file_name().unwrap()
                .to_string_lossy()
                .trim_end_matches(".stud")
                .to_string();
            ListItem::new(name)
        })
        .collect();
    let max_item_width = items.iter().map(|i| i.width()).max().unwrap_or(5) as u16 + 3;

    // Create the layout
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(max_item_width), Constraint::Fill(5), Constraint::Fill(2)])
        .spacing(1)
        .split(f.area());

    // Render sidebar
    let items = List::new(items)
        .block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_type(BorderType::QuadrantInside)
                .padding(Padding::right(1))
        )
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::Black));

    f.render_stateful_widget(items, chunks[0], &mut app.list_state);

    // Render main view
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
    let modes = &[Mode::Program, Mode::Readme];
    let line = Line::from_iter(
        modes.iter().flat_map(|&mode| {
            let style =
                if mode == app.mode {
                    Style::default().bg(Color::Blue).add_modifier(Modifier::BOLD)
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

    // Render approval toggle
    let approval_text = match app.approval() {
        Some(true) => "✓ Approved",
        Some(false) => "✗ Not Approved",
        None => "? Unchecked",
    };

    let approval_style = match app.approval() {
        Some(true) => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        Some(false) => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        None => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    };

    let approval_widget = Paragraph::new(approval_text)
        .style(approval_style)
        .block(Block::default().borders(Borders::TOP).title("Status"));

    f.render_widget(approval_widget, main_chunks[1]);

    // Render README content with scrollbar
    let content_area = main_chunks[2];

    // Calculate visible lines
    let inner_height = content_area.height.saturating_sub(2) as usize; // -2 for borders
    let lines: Vec<Line> = app
        .content()
        .lines()
        .enumerate()
        .skip(app.scroll_offset)
        .take(inner_height)
        .map(|(lineno, line)| Line::from(vec![
            Span::styled(
                format!("{lineno: >4}  "),
                Style::default().fg(Color::Magenta).add_modifier(Modifier::ITALIC)
            ),
            Span::raw(line.to_string()),
        ]))
        .collect();

    let paragraph = Paragraph::new(lines)
        .wrap(Default::default())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("README")
                .padding(Padding::new(1, 1, 1, 1))
        );

    f.render_widget(paragraph, content_area);

    // Render scrollbar
    let total_lines = app.content().lines().count();
    if total_lines > inner_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state = ScrollbarState::new(total_lines.saturating_sub(inner_height))
            .position(app.scroll_offset);

        f.render_stateful_widget(
            scrollbar,
            content_area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn render_comments(f: &mut Frame, app: &mut App, area: Rect) {
    let mode_widget = Paragraph::new(Text::from(app.comments.as_str()))
        .wrap(Default::default())
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_type(BorderType::QuadrantInside)
                .padding(Padding::left(1))
        );
    f.render_widget(mode_widget, area);
}
