use std::io::{self, Write};

use rocket_tycoon::game_state::GameState;
use rocket_tycoon::ui::App;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let (company_name, seed) = if args.len() >= 2 {
        let name = args[1].clone();
        let seed = args.get(2)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_else(|| rand::random());
        (name, seed)
    } else {
        prompt_new_game()?
    };

    let game = GameState::new(company_name, 200_000_000.0, seed);
    let mut app = App::new(game);
    app.run()
}

fn prompt_new_game() -> io::Result<(String, u64)> {
    print!("Company name: ");
    io::stdout().flush()?;
    let mut name = String::new();
    io::stdin().read_line(&mut name)?;
    let name = name.trim().to_string();
    let name = if name.is_empty() { "SpaceCorp".to_string() } else { name };

    print!("Seed (blank for random): ");
    io::stdout().flush()?;
    let mut seed_str = String::new();
    io::stdin().read_line(&mut seed_str)?;
    let seed = seed_str.trim().parse::<u64>().unwrap_or_else(|_| rand::random());

    println!("Starting {} with seed {}...", name, seed);
    Ok((name, seed))
}
