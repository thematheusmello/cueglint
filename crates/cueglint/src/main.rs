mod app;
mod runtime;
mod worker;

use std::path::PathBuf;

#[derive(Default)]
struct Options {
    database: Option<PathBuf>,
    fixture: Option<String>,
    screenshot: Option<PathBuf>,
    overlay: bool,
}

fn options() -> anyhow::Result<Options> {
    let mut result = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--database" => {
                result.database = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--database requires a path"))?
                        .into(),
                )
            }
            "--fixture" => {
                let fixture = args.next().ok_or_else(|| {
                    anyhow::anyhow!("--fixture requires dashboard, overlay, history, or session")
                })?;
                anyhow::ensure!(
                    ["dashboard", "overlay", "history", "session"].contains(&fixture.as_str()),
                    "unknown fixture: {fixture}"
                );
                result.overlay = fixture == "overlay" || fixture == "history";
                result.fixture = Some(fixture);
            }
            "--screenshot" => {
                result.screenshot = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--screenshot requires a PNG path"))?
                        .into(),
                )
            }
            "--overlay" => result.overlay = true,
            "--help" | "-h" => {
                println!(
                    "Cueglint — native local meeting assistant\n\nUsage: cueglint [--database PATH] [--overlay] [--fixture dashboard|overlay|history|session] [--screenshot PATH]\n\nFixtures use an isolated in-memory database and visibly labelled sample data.\nLocal capture and inference require model setup and OS permissions. Fixtures never capture, infer, or download."
                );
                std::process::exit(0);
            }
            _ => anyhow::bail!("unknown argument: {arg}; use --help"),
        }
    }
    anyhow::ensure!(
        result.fixture.is_none() || result.database.is_none(),
        "fixtures cannot use a persistent database"
    );
    Ok(result)
}

fn main() -> anyhow::Result<()> {
    let options = options()?;
    let assistant_fixture = options
        .fixture
        .as_deref()
        .is_some_and(|fixture| fixture != "dashboard");
    let viewport = if assistant_fixture {
        let height = if options.fixture.as_deref() == Some("overlay") {
            104.0
        } else {
            760.0
        };
        eframe::egui::ViewportBuilder::default()
            .with_title("Cueglint [Fixture]")
            .with_inner_size([690.0, height])
            .with_min_inner_size([690.0, height])
            .with_max_inner_size([690.0, height])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
    } else {
        eframe::egui::ViewportBuilder::default()
            .with_title("Cueglint")
            .with_inner_size([860.0, 670.0])
            .with_min_inner_size([860.0, 670.0])
            .with_fullsize_content_view(true)
            .with_title_shown(false)
            .with_titlebar_shown(false)
    };
    let native = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        persist_window: options.fixture.is_none(),
        persistence_path: options.fixture.as_ref().map(|view| {
            std::env::temp_dir()
                .join("cueglint-native-fixtures")
                .join(view)
        }),
        ..Default::default()
    };
    eframe::run_native(
        "Cueglint",
        native,
        Box::new(move |cc| Ok(Box::new(app::Cueglint::new(cc, options)))),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}
