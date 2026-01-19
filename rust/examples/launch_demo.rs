use rocket_tycoon::launcher::{LaunchSimulator, LaunchStage};

fn main() {
    println!("=== Rocket Launch Simulator Demo ===\n");

    // Run 5 simulations
    for attempt in 1..=5 {
        println!("Launch Attempt #{}", attempt);
        println!("{}", "-".repeat(40));

        let result = LaunchSimulator::simulate_launch_with_callback(|stage| {
            println!("  ✓ {} - PASSED", stage.description());
        });

        println!("\n{}\n", result.message());
    }

    // Statistics over many launches
    println!("\n=== Statistics (1000 launches) ===");
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut failure_stages = vec![0; LaunchStage::all_stages().len()];

    for _ in 0..1000 {
        match LaunchSimulator::simulate_launch() {
            rocket_tycoon::launcher::LaunchResult::Success => success_count += 1,
            rocket_tycoon::launcher::LaunchResult::Failure { stage } => {
                failure_count += 1;
                let stage_index = LaunchStage::all_stages()
                    .iter()
                    .position(|s| s == &stage)
                    .unwrap();
                failure_stages[stage_index] += 1;
            }
        }
    }

    println!("Successes: {} ({:.1}%)", success_count, success_count as f64 / 10.0);
    println!("Failures:  {} ({:.1}%)", failure_count, failure_count as f64 / 10.0);
    println!("\nFailures by stage:");
    for (i, stage) in LaunchStage::all_stages().iter().enumerate() {
        if failure_stages[i] > 0 {
            println!("  {}: {} ({:.1}%)",
                stage.description(),
                failure_stages[i],
                failure_stages[i] as f64 / 10.0
            );
        }
    }
}
