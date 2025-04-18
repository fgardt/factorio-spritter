use std::process::ExitCode;

use clap::Parser;

#[macro_use]
extern crate log;

mod commands;
mod image_util;
mod logger;
mod lua;

use commands::{generate_gif, generate_mipmap_icon, optimize, GenerationCommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about=None)]
struct Cli {
    #[clap(subcommand)]
    command: GenerationCommand,
}

fn main() -> ExitCode {
    let args = Cli::parse();
    logger::init("info,oxipng=warn");
    info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let res = match args.command {
        GenerationCommand::Spritesheet { args } => args.execute(),
        GenerationCommand::Icon { args } => generate_mipmap_icon(&args),
        GenerationCommand::Gif { args } => generate_gif(&args),
        GenerationCommand::Optimize { args } => optimize(&args),
    };

    if let Err(err) = res {
        error!("{err}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
