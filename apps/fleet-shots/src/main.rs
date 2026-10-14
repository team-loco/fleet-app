use std::path::PathBuf;

use clap::Parser;
use fleet::{AppearanceArg, Object, Preset, Screen, parse_object};
use fleet_core::ClusterId;
use fleet_core::app_settings::Highlight;
use fleet_shots::Highlights;

#[derive(Parser, Debug)]
#[command(
    name = "fleet-shots",
    about = "Render a Fleet demo screen to a PNG without a window"
)]
struct Args {
    #[arg(value_enum)]
    screen: Screen,
    #[arg(value_enum, default_value_t = AppearanceArg::Dark)]
    appearance: AppearanceArg,
    #[arg(long, value_name = "PATH")]
    out: PathBuf,
    #[arg(long, value_enum)]
    state: Option<fleet_shots::StateFixture>,
    #[arg(long, value_name = "NAME")]
    cluster: Option<String>,
    #[arg(long, value_name = "NAMESPACE/NAME", value_parser = parse_object)]
    object: Option<Object>,
    #[arg(long, value_name = "PRESET|#HEX", value_parser = parse_highlight)]
    primary: Option<Highlight>,
    #[arg(long, value_name = "PRESET|#HEX", value_parser = parse_highlight)]
    secondary: Option<Highlight>,
}

fn parse_highlight(input: &str) -> Result<Highlight, String> {
    let preset = Highlight::PRESETS
        .into_iter()
        .find(|preset| preset.label().eq_ignore_ascii_case(input));
    match preset {
        Some(preset) => Ok(preset),
        None => input
            .parse()
            .map(Highlight::Custom)
            .map_err(|error| format!("{input}: a preset name or {error}")),
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let preset = Preset {
        cluster: args.cluster.map(ClusterId::new),
        object: args.object,
    };
    let appearance = args.appearance.into();
    let highlights = Highlights {
        primary: args.primary.unwrap_or_default(),
        secondary: args.secondary.unwrap_or_default(),
    };
    let image =
        fleet_shots::render_state(args.screen, &preset, appearance, highlights, args.state)?;
    fleet_shots::save(&image, &args.out)?;
    println!("{}", args.out.display());
    Ok(())
}
