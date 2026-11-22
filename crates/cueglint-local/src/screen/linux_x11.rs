//! Explicit local X11 monitor capture. Pixel reads only occur in capture().
use super::{Display, ScreenCapture, capture_dimensions, linux_ocr};
use anyhow::{Context, Result, ensure};
use image::{Rgb, RgbImage};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use x11rb::{
    connection::Connection,
    image::{Image, PixelLayout},
    protocol::{
        randr::{ConnectionExt as _, SetConfig},
        render::{ConnectionExt as _, CreatePictureAux, PictOp, Transform},
        xproto::{ConnectionExt as _, Screen, SubwindowMode, VisualClass},
    },
    rust_connection::RustConnection,
};

fn local_address(value: &str) -> Result<String> {
    let tail = value
        .strip_prefix("unix/:")
        .or_else(|| value.strip_prefix("unix:"))
        .or_else(|| value.strip_prefix(':'))
        .context("Screen capture requires a local X11 Unix socket")?;
    let (display, screen) = tail.split_once('.').unwrap_or((tail, "0"));
    let number = |s: &str| -> Result<u16> {
        ensure!(
            !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()),
            "Invalid local X11 display"
        );
        Ok(s.parse()?)
    };
    // An explicitly named Unix transport prevents x11rb's ordinary fallback to localhost TCP.
    Ok(format!("unix/:{}.{}", number(display)?, number(screen)?))
}

pub(crate) fn session_address() -> Result<String> {
    ensure!(
        std::env::var_os("WAYLAND_DISPLAY").is_none()
            && std::env::var_os("WAYLAND_SOCKET").is_none()
            && !std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s.eq_ignore_ascii_case("wayland")),
        "Wayland screen capture requires a portal; this build supports local X11 displays"
    );
    local_address(&std::env::var("DISPLAY").context("No local X11 display is configured")?)
}

#[derive(Debug, PartialEq, Eq)]
struct Monitor {
    id: u32,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    primary: bool,
}

impl Monitor {
    fn descriptor(&self) -> Display {
        Display {
            id: self.id,
            width: self.width.into(),
            height: self.height.into(),
        }
    }
}

struct Desktop {
    conn: RustConnection,
    index: usize,
    address: String,
}

impl Desktop {
    fn connect() -> Result<Self> {
        let address = session_address()?;
        let (conn, index) = RustConnection::connect(Some(&address))
            .context("Cannot connect to the local X11 display with the current X authority")?;
        ensure!(
            !conn.query_extension(b"XWAYLAND")?.reply()?.present,
            "Xwayland does not provide the complete display; native Wayland capture requires a portal"
        );
        let version = conn
            .randr_query_version(1, 5)?
            .reply()
            .context("X11 display capture requires RandR 1.5")?;
        ensure!(
            (version.major_version, version.minor_version) >= (1, 5),
            "X11 display capture requires RandR 1.5"
        );
        Ok(Self {
            conn,
            index,
            address,
        })
    }

    fn screen(&self) -> &Screen {
        &self.conn.setup().roots[self.index]
    }

    fn monitors(&self) -> Result<Vec<Monitor>> {
        let reply = self
            .conn
            .randr_get_monitors(self.screen().root, true)?
            .reply()?;
        ensure!(
            reply.monitors.len() <= 32,
            "X11 monitor catalog exceeds the supported limit"
        );
        let config_time = self
            .conn
            .randr_get_screen_resources_current(self.screen().root)?
            .reply()?
            .config_timestamp;
        let edid_atom = self.conn.intern_atom(true, b"EDID")?.reply()?.atom;
        let mut monitors = Vec::new();
        for m in reply.monitors {
            let name = self.conn.get_atom_name(m.name)?.reply()?.name;
            ensure!(!name.is_empty(), "X11 monitor has no stable name");
            let mut hash = Sha256::new();
            hash.update(b"cueglint-x11-monitor-v1\0");
            hash.update(self.address.as_bytes());
            hash.update([0]);
            hash.update((name.len() as u32).to_be_bytes());
            hash.update(name);
            ensure!(
                m.outputs.len() <= 128,
                "Too many outputs for one X11 monitor"
            );
            let mut outputs = Vec::new();
            for output in m.outputs {
                let info = self
                    .conn
                    .randr_get_output_info(output, config_time)?
                    .reply()?;
                ensure!(
                    info.status == SetConfig::SUCCESS,
                    "X11 outputs changed while identifying the monitor"
                );
                let mut fingerprint = Sha256::new();
                fingerprint.update((info.name.len() as u32).to_be_bytes());
                fingerprint.update(info.name);
                if edid_atom != 0 {
                    let property = self
                        .conn
                        .randr_get_output_property(output, edid_atom, 0u32, 0, 8192, false, false)?
                        .reply()?;
                    ensure!(
                        property.bytes_after == 0,
                        "X11 output identity exceeds 32 KiB"
                    );
                    fingerprint.update(property.data);
                }
                outputs.push(fingerprint.finalize());
            }
            outputs.sort_unstable();
            for output in outputs {
                hash.update(output);
            }
            let id = u32::from_be_bytes(hash.finalize()[..4].try_into()?).max(1);
            ensure!(
                !monitors.iter().any(|v: &Monitor| v.id == id),
                "X11 monitor identities are ambiguous"
            );
            monitors.push(Monitor {
                id,
                x: m.x,
                y: m.y,
                width: m.width,
                height: m.height,
                primary: m.primary,
            });
        }
        Ok(monitors)
    }

    fn selected(&self, id: Option<u32>) -> Result<Monitor> {
        choose_monitor(self.monitors()?, id)
    }

    fn pixels(&self, m: &Monitor) -> Result<(RgbImage, i64)> {
        let c = &self.conn;
        let screen = self.screen();
        let geometry = c.get_geometry(screen.root)?.reply()?;
        ensure!(
            m.x >= 0
                && m.y >= 0
                && u32::from(m.x as u16) + u32::from(m.width) <= u32::from(geometry.width)
                && u32::from(m.y as u16) + u32::from(m.height) <= u32::from(geometry.height),
            "Selected monitor lies outside the current X11 root geometry"
        );
        let (w, h) = capture_dimensions(f64::from(m.width), f64::from(m.height))?;
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|d| &d.visuals)
            .find(|v| v.visual_id == screen.root_visual)
            .context("Missing X11 root visual")?;
        ensure!(
            visual.class == VisualClass::TRUE_COLOR,
            "X11 monitor capture requires a TrueColor visual"
        );
        let layout = PixelLayout::from_visual_type(*visual)?;
        let (w, h) = (u16::try_from(w)?, u16::try_from(h)?);
        let (image, _) = if (w, h) == (m.width, m.height) {
            Image::get(c, screen.root, m.x, m.y, w, h)?
        } else {
            // Composite one frame into a bounded server-side pixmap before transferring pixels.
            let formats = c.render_query_pict_formats()?.reply()?;
            let format = formats
                .screens
                .get(self.index)
                .into_iter()
                .flat_map(|s| &s.depths)
                .flat_map(|d| &d.visuals)
                .find(|v| v.visual == screen.root_visual)
                .context("Missing XRender format for the selected monitor")?
                .format;
            let pixmap = c.generate_id()?;
            c.create_pixmap(screen.root_depth, pixmap, screen.root, w, h)?
                .check()?;
            let source = c.generate_id()?;
            let destination = c.generate_id()?;
            c.render_create_picture(
                source,
                screen.root,
                format,
                &CreatePictureAux::new().subwindowmode(SubwindowMode::INCLUDE_INFERIORS),
            )?
            .check()?;
            c.render_create_picture(destination, pixmap, format, &CreatePictureAux::new())?
                .check()?;
            let ratio = |src: u16, dest: u16| (i64::from(src) * 65536 / i64::from(dest)) as i32;
            let transform = Transform {
                matrix11: ratio(m.width, w),
                matrix12: 0,
                matrix13: i32::from(m.x) << 16,
                matrix21: 0,
                matrix22: ratio(m.height, h),
                matrix23: i32::from(m.y) << 16,
                matrix31: 0,
                matrix32: 0,
                matrix33: 1 << 16,
            };
            c.render_set_picture_transform(source, transform)?.check()?;
            c.render_set_picture_filter(source, b"nearest", &[])?
                .check()?;
            c.render_composite(
                PictOp::SRC,
                source,
                0u32,
                destination,
                0,
                0,
                0,
                0,
                0,
                0,
                w,
                h,
            )?
            .check()?;
            Image::get(c, pixmap, 0, 0, w, h)?
        };
        // Closing this private connection releases every allocated X resource, including on error.
        let captured_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            .try_into()?;
        let pixels = RgbImage::from_fn(w.into(), h.into(), |x, y| {
            let (r, g, b) = layout.decode(image.get_pixel(x as u16, y as u16));
            Rgb([(r >> 8) as u8, (g >> 8) as u8, (b >> 8) as u8])
        });
        Ok((pixels, captured_at))
    }
}

fn choose_monitor(monitors: Vec<Monitor>, id: Option<u32>) -> Result<Monitor> {
    let single = monitors.len() == 1;
    let mut matches = monitors.into_iter().filter(|monitor| match id {
        Some(id) => monitor.id == id,
        None => single || monitor.primary,
    });
    let chosen = matches.next().with_context(|| match id {
        Some(id) => format!("Display {id} is unavailable; reconnect it or select another display"),
        None => "No primary X11 monitor; select an available display in Settings".into(),
    })?;
    ensure!(
        matches.next().is_none(),
        "Ambiguous X11 selection; choose a display in Settings"
    );
    Ok(chosen)
}

pub(super) fn displays() -> Result<Vec<Display>> {
    Ok(Desktop::connect()?
        .monitors()?
        .iter()
        .map(Monitor::descriptor)
        .collect())
}

pub(super) fn capture(id: Option<u32>) -> Result<ScreenCapture> {
    let desktop = Desktop::connect()?;
    let monitor = desktop.selected(id)?;
    let (image, captured_at) = desktop.pixels(&monitor)?;
    ensure!(
        desktop.selected(id)? == monitor,
        "X11 monitor changed during capture; retry with the current display"
    );
    drop(desktop);
    let text = linux_ocr::recognize(&image)?;
    let (width, height) = image.dimensions();
    let mut encoded = Cursor::new(Vec::new());
    image.write_to(&mut encoded, image::ImageFormat::Png)?;
    let png = encoded.into_inner();
    Ok(ScreenCapture {
        id: uuid::Uuid::new_v4().to_string(),
        display_id: monitor.id,
        captured_at,
        width,
        height,
        sha256: format!("{:x}", Sha256::digest(&png)),
        png,
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_selection_requires_a_single_monitor_or_one_primary() {
        let monitor = |id, primary| Monitor {
            id,
            primary,
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        };
        assert_eq!(choose_monitor(vec![monitor(1, false)], None).unwrap().id, 1);
        assert_eq!(
            choose_monitor(vec![monitor(1, false), monitor(2, true)], None)
                .unwrap()
                .id,
            2
        );
        assert_eq!(
            choose_monitor(vec![monitor(1, false), monitor(2, true)], Some(1))
                .unwrap()
                .id,
            1
        );
        assert!(choose_monitor(vec![monitor(1, true)], Some(2)).is_err());
        assert!(choose_monitor(vec![monitor(1, false), monitor(2, false)], None).is_err());
        assert!(choose_monitor(vec![monitor(1, true), monitor(2, true)], None).is_err());
        assert!(choose_monitor(vec![], None).is_err());
    }

    #[test]
    fn local_only_transport_never_falls_back_to_tcp_or_parses_a_foreign_socket() {
        for address in [":04", "unix/:4.0", "unix:4"] {
            assert_eq!(local_address(address).unwrap(), "unix/:4.0");
        }
        assert_eq!(local_address(":1.2").unwrap(), "unix/:1.2");
        for address in [
            "",
            ":",
            "tcp/localhost:0",
            "localhost:0",
            "remote:1.2",
            ":3.4.5",
            "unix/:../server",
            ":99999",
        ] {
            assert!(local_address(address).is_err(), "{address}");
        }
    }
}
