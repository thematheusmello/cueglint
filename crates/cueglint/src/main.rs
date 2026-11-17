mod app;
mod brand;
mod markdown;
mod reply_window;
mod runtime;
mod screenshot;
mod shortcuts;
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
                Some("overlay" | "history" | "session" | "markdown" | "markdown-long")
            )
    }

    fn fixture_viewport_size(&self) -> Option<[f32; 2]> {
        match self.fixture.as_deref() {
            Some("overlay") => Some([690.0, 104.0]),
            Some("history" | "session") => Some([690.0, 760.0]),
            Some("markdown" | "markdown-long") => Some([690.0, 500.0]),
            _ => None,
        }
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
                    anyhow::anyhow!("--fixture requires dashboard, overlay, history, session, markdown, or markdown-long")
                })?;
                anyhow::ensure!(
                    [
                        "dashboard",
                        "overlay",
                        "history",
                        "session",
                        "markdown",
                        "markdown-long"
                    ]
                    .contains(&fixture.as_str()),
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
                    "Cueglint — native local meeting assistant\n\nUsage: cueglint [--database PATH] [--settings | --overlay] [--fixture dashboard|overlay|history|session|markdown|markdown-long] [--screenshot PATH]\n\nBy default, the compact assistant opens. Its gear opens Settings; --settings opens Settings directly.\nScreenshots require --settings or a labelled fixture and refuse existing paths. A saved screenshot emits one JSON receipt with actual image dimensions and rendering metrics.\n\nFixtures use an isolated in-memory database and visibly labelled sample data. markdown and markdown-long show original short and 80-item answers in a 690×500 primary viewport.\nLocal capture and inference require model setup and OS permissions. Fixtures never capture, infer, or download."
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
    let viewport = if let Some(size) = options.fixture_viewport_size() {
        eframe::egui::ViewportBuilder::default()
            .with_title("Cueglint [Fixture]")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
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
    let viewport = viewport.with_icon(brand::app_icon());
    let (shortcut_bridge, shortcut_events) = shortcuts::channel();
    let mut native = eframe::NativeOptions {
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
    shortcuts::configure_native(&mut native, &shortcut_bridge);
    eframe::run_native(
        "Cueglint",
        native,
        Box::new(move |cc| {
            let fixture = options.fixture.is_some();
            let mut app = app::Cueglint::new(cc, options);
            app.install_shortcuts(shortcuts::Manager::native(
                shortcut_bridge,
                shortcut_events,
                cc.egui_ctx.clone(),
                fixture,
            ));
            Ok(Box::new(app))
        }),
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
        for view in [
            "dashboard",
            "overlay",
            "history",
            "session",
            "markdown",
            "markdown-long",
        ] {
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

    #[test]
    fn labelled_markdown_fixtures_use_the_conversation_primary_viewport() {
        for view in ["markdown", "markdown-long"] {
            let options = parse(&["--fixture", view]).unwrap();
            assert_eq!(options.fixture_viewport_size(), Some([690.0, 500.0]));
            assert!(options.assistant_on_start());
            assert!(parse(&["--fixture", view, "--database", "/tmp/private.sqlite3"]).is_err());
        }
        assert_eq!(
            parse(&["--fixture", "overlay"])
                .unwrap()
                .fixture_viewport_size(),
            Some([690.0, 104.0])
        );
        for view in ["history", "session"] {
            assert_eq!(
                parse(&["--fixture", view]).unwrap().fixture_viewport_size(),
                Some([690.0, 760.0])
            );
        }
        assert_eq!(
            parse(&["--fixture", "dashboard"])
                .unwrap()
                .fixture_viewport_size(),
            None
        );
    }
}
