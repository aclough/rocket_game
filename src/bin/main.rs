use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use rocket_tycoon::game_state::GameState;
use rocket_tycoon::save;
use rocket_tycoon::ui::App;

enum StartupState {
    Menu,
    NameInput,
}

fn main() -> io::Result<()> {
    let game = if std::env::args().len() >= 2 {
        let args: Vec<String> = std::env::args().collect();
        let name = args[1].clone();
        let seed = args
            .get(2)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_else(|| rand::random());
        GameState::with_balance(name, seed, rocket_tycoon::balance_config::BalanceConfig::default())
    } else {
        run_startup_screen()?
    };
    let mut app = App::new(game);
    app.run()
}

fn run_startup_screen() -> io::Result<GameState> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = startup_loop(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn startup_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<GameState> {
    let mut state = StartupState::Menu;
    let mut selected: usize = 0;
    let mut saves = save::list_saves();
    let mut company_name = String::new();

    loop {
        let menu_len = 1 + saves.len(); // "New Game" + saved games

        terminal.draw(|frame| match &state {
            StartupState::Menu => draw_menu(frame, &saves, selected),
            StartupState::NameInput => draw_name_input(frame, &company_name),
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match &state {
                StartupState::Menu => match key.code {
                    KeyCode::Char('q') => {
                        return Err(io::Error::new(io::ErrorKind::Interrupted, "quit"));
                    }
                    KeyCode::Up => {
                        if selected > 0 {
                            selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if selected + 1 < menu_len {
                            selected += 1;
                        }
                    }
                    KeyCode::Enter => {
                        if selected == 0 {
                            // New Game
                            company_name.clear();
                            state = StartupState::NameInput;
                        } else {
                            // Load saved game
                            let idx = selected - 1;
                            if idx < saves.len() {
                                let path = &saves[idx].1;
                                return save::load_game(path);
                            }
                        }
                    }
                    _ => {}
                },
                StartupState::NameInput => match key.code {
                    KeyCode::Enter => {
                        let name = if company_name.trim().is_empty() {
                            "SpaceCorp".to_string()
                        } else {
                            company_name.trim().to_string()
                        };
                        let seed: u64 = rand::random();
                        return Ok(GameState::with_balance(name, seed, rocket_tycoon::balance_config::BalanceConfig::default()));
                    }
                    KeyCode::Esc => {
                        state = StartupState::Menu;
                        saves = save::list_saves(); // refresh
                    }
                    KeyCode::Backspace => {
                        company_name.pop();
                    }
                    KeyCode::Char(c) => {
                        if company_name.len() < 30 {
                            company_name.push(c);
                        }
                    }
                    _ => {}
                },
            }
        }
    }
}

fn draw_menu(frame: &mut Frame, saves: &[(String, std::path::PathBuf)], selected: usize) {
    let area = frame.area();

    // Center the content
    let content_width = 40u16;
    let content_height = (8 + saves.len() as u16).min(area.height);
    let x = area.width.saturating_sub(content_width) / 2;
    let y = area.height.saturating_sub(content_height) / 3;
    let content_area = Rect::new(x, y, content_width.min(area.width), content_height);

    let mut constraints = vec![
        Constraint::Length(3), // title
        Constraint::Length(1), // blank
        Constraint::Length(1), // New Game
    ];
    if !saves.is_empty() {
        constraints.push(Constraint::Length(1)); // blank
        constraints.push(Constraint::Length(1)); // "Saved Games" header
        constraints.push(Constraint::Min(1)); // save list
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(content_area);

    // Title
    let title = Paragraph::new("Rocket Tycoon")
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Double));
    frame.render_widget(title, chunks[0]);

    // New Game option
    let marker = if selected == 0 { "> " } else { "  " };
    let style = if selected == 0 {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let new_game = Paragraph::new(format!("{}New Game", marker)).style(style);
    frame.render_widget(new_game, chunks[2]);

    // Saved games
    if !saves.is_empty() {
        let header = Paragraph::new("── Saved Games ──")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(header, chunks[4]);

        let items: Vec<ListItem> = saves
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                let idx = i + 1;
                let marker = if selected == idx { "> " } else { "  " };
                let style = if selected == idx {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{}{}", marker, name)).style(style)
            })
            .collect();
        let list = List::new(items);
        frame.render_widget(list, chunks[5]);
    }
}

fn draw_name_input(frame: &mut Frame, name: &str) {
    let area = frame.area();

    let content_width = 40u16;
    let content_height = 5u16;
    let x = area.width.saturating_sub(content_width) / 2;
    let y = area.height.saturating_sub(content_height) / 3;
    let content_area = Rect::new(x, y, content_width.min(area.width), content_height);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // label + input
            Constraint::Length(1), // blank
            Constraint::Length(1), // hint
        ])
        .split(content_area);

    let input_text = format!("Company name: {}█", name);
    let input = Paragraph::new(input_text).style(Style::default().fg(Color::White));
    frame.render_widget(input, chunks[0]);

    let hint = Paragraph::new("[Enter] Start  [Esc] Back")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, chunks[2]);
}
