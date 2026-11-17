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
    settings: bool,
}

impl Options {
    fn assistant_on_start(&self) -> bool {
        self.overlay
            || (self.fixture.is_none() && !self.settings)
            || matches!(
                self.fixture.as_deref(),
                Some("overlay" | "history" | "session")
            )
    }
}

fn options() -> anyhow::Result<Options> {
    parse_options(std::env::args().skip(1))
}

fn parse_options(mut args: impl Iterator<Item = String>) -> anyhow::Result<Options> {
    let mut result = Options::default();
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
            "--settings" => result.settings = true,
            "--help" | "-h" => {
                println!(
                    "Cueglint — native local meeting assistant\n\nUsage: cueglint [--database PATH] [--settings | --overlay] [--fixture dashboard|overlay|history|session] [--screenshot PATH]\n\nBy default, the compact assistant opens. Its gear opens Settings; --settings opens Settings directly.\nScreenshots require --settings or a labelled fixture.\n\nFixtures use an isolated in-memory database and visibly labelled sample data.\nLocal capture and inference require model setup and OS permissions. Fixtures never capture, infer, or download."
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
    anyhow::ensure!(
        !(result.settings && result.overlay),
        "choose --settings or --overlay"
    );
    anyhow::ensure!(
        result.screenshot.is_none() || result.fixture.is_some() || result.settings,
        "--screenshot requires --settings or --fixture; child-window readback is not supported"
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> anyhow::Result<Options> {
        parse_options(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn startup_is_compact_with_an_explicit_settings_review_path() {
        assert!(parse(&[]).unwrap().assistant_on_start());
        assert!(!parse(&["--settings"]).unwrap().assistant_on_start());
        assert!(parse(&["--overlay"]).unwrap().assistant_on_start());
        assert!(parse(&["--settings", "--overlay"]).is_err());
        for view in ["dashboard", "overlay", "history", "session"] {
            let options = parse(&[
                "--fixture",
                view,
                "--screenshot",
                "/tmp/cueglint-fixture.png",
            ])
            .unwrap();
            assert_eq!(options.assistant_on_start(), view != "dashboard");
        }
    }

    #[test]
    fn screenshots_require_a_visible_primary_viewport() {
        assert!(parse(&["--screenshot", "/tmp/cueglint.png"]).is_err());
        assert!(parse(&["--settings", "--screenshot", "/tmp/cueglint.png"]).is_ok());
        assert!(parse(&["--fixture", "overlay", "--database", "/tmp/user.sqlite3"]).is_err());
    }
}
