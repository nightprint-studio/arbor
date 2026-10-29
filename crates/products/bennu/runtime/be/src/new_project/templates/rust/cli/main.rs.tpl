use clap::Parser;

/// {{title}}
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Who to greet
    #[arg(short, long, default_value = "world")]
    name: String,

    /// Repeat the greeting
    #[arg(short, long, default_value_t = 1)]
    count: u8,
}

fn main() {
    let cli = Cli::parse();
    for _ in 0..cli.count {
        println!("Hello, {}!", cli.name);
    }
}
