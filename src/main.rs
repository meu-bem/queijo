mod app;
mod capture;
mod cli;
mod config;
mod desktop;
mod doctor;
mod gif;
mod i18n;
mod icon;
mod monitors;
mod output;
mod paint;
mod portal;
mod record;
mod settings;
mod shell_ext;
mod shortcut;

use std::io::Write;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use eframe::egui;

fn main() {
    let args = cli::Cli::parse();

    let mut cfg = config::Config::load();
    i18n::set_language(&cfg.language);

    if args.settings {
        if let Err(e) = settings::run() {
            fail(&i18n::tf("error.open_settings", &[("error", &e.to_string())]));
        }
        return;
    }

    if args.doctor {
        doctor::run();
        return;
    }

    output::set_notifications(cfg.notifications);
    if let Some(area) = &args.record {
        record_area(area, &cfg);
    }
    let software_gl = std::env::var("QUEIJO_SOFTWARE_GL").ok();
    if desktop::wants_software_gl(desktop::Env::current(), software_gl.as_deref())
        && std::env::var_os("LIBGL_ALWAYS_SOFTWARE").is_none()
    {
        // Mutter (42) scans a fullscreen GPU buffer straight out to the monitor,
        // ignoring its alpha, so our see-through windows would show up black.
        // Software rendering hands it shared-memory buffers, which it always
        // composites. Set before any thread starts, as setting the environment
        // isn't thread-safe.
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
    }
    if let Some(dir) = &args.output_dir {
        cfg.screenshots.folder = dir.to_string_lossy().into_owned();
    }
    match args.kind.as_deref() {
        Some("photo") => cfg.start_kind = config::StartKind::Screenshot,
        Some("video") => cfg.start_kind = config::StartKind::Video,
        Some("gif") => cfg.start_kind = config::StartKind::Gif,
        _ => {}
    }
    match args.area.as_deref() {
        Some("selection") => cfg.start_area = config::StartArea::Selection,
        Some("screen") => cfg.start_area = config::StartArea::Screen,
        Some("window") => cfg.start_area = config::StartArea::Window,
        _ => {}
    }

    let mut monitors = monitors::detect();
    if args.list_monitors {
        for m in &monitors {
            println!("{}: {}  {}x{}+{}+{}", m.index, m.label(), m.width, m.height, m.x, m.y);
        }
        return;
    }

    // Pressing the hotkey again while we run stops a recording instead of
    // stacking a second overlay on top of the first.
    let Some(_instance) = single_instance() else {
        if !signal_running() {
            eprintln!("queijo: another instance is starting");
        }
        return;
    };
    if args.stop {
        eprintln!("queijo: not recording");
        return;
    }

    if args.delay > 0 {
        std::thread::sleep(Duration::from_secs(args.delay));
    }

    if args.quick {
        let (desktop, _) = capture::capture().unwrap_or_else(|e| fail(&i18n::tf("error.capture", &[("error", &e)])));
        let image = match &args.monitor {
            Some(_) if !monitors.is_empty() => {
                let pos = pick_monitor(&monitors, args.monitor.as_deref());
                capture::split(&desktop, &monitors).swap_remove(pos)
            }
            _ => desktop,
        };
        match output::save_image(&image, &cfg.screenshots, cfg.image_format, cfg.jpeg_quality) {
            Ok(path) => println!("{}", path.display()),
            Err(e) => fail(&i18n::tf("error.save_screenshot", &[("error", &e)])),
        }
        return;
    }

    // With the Queijo Shell extension we can offer every window; otherwise only
    // the focused one, found by matching its picture on the screen.
    let extension = zbus::blocking::Connection::session()
        .ok()
        .and_then(|conn| shell_ext::list_windows(&conn).map(|list| (conn, list)));
    let shots_job = extension.map(|(conn, list)| std::thread::spawn(move || shell_ext::capture_all(&conn, list)));
    let window_taken = Arc::new(AtomicBool::new(shots_job.is_some()));

    // The screen is captured while our windows start up. They stay fully
    // transparent until it is done, so they never end up in the picture, and
    // afterwards only dim the live screen instead of showing a copy of it.
    let all_monitors = monitors.clone();
    let start_capture = {
        let taken = window_taken.clone();
        move |shown: Vec<monitors::Monitor>| {
            std::thread::spawn(move || {
                let shot = capture::capture_with_window(&taken)?;
                Ok(prepare(shot, &all_monitors, &shown))
            })
        }
    };

    let (capture_job, desktop_rect) = if monitors.is_empty() {
        // Nothing to size the windows by but the picture itself.
        let job = start_capture(Vec::new());
        let captured: Result<app::Captured, String> = job.join().unwrap_or_else(|_| Err("capture panicked".into()));
        let captured = captured.unwrap_or_else(|e| fail(&i18n::tf("error.capture", &[("error", &e)])));
        let (w, h) = captured.desktop.dimensions();
        monitors.push(monitors::Monitor {
            name: "screen".into(),
            width: w as i32,
            height: h as i32,
            ..Default::default()
        });
        let captured = app::Captured { images: vec![(*captured.desktop).clone()], ..captured };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w as f32, h as f32));
        (std::thread::spawn(move || Ok(captured)), rect)
    } else {
        let min_x = monitors.iter().map(|m| m.x).min().unwrap_or(0) as f32;
        let min_y = monitors.iter().map(|m| m.y).min().unwrap_or(0) as f32;
        let max_x = monitors.iter().map(|m| m.x + m.width).max().unwrap_or(0) as f32;
        let max_y = monitors.iter().map(|m| m.y + m.height).max().unwrap_or(0) as f32;
        let rect = egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y));
        if args.monitor.is_some() {
            let pos = pick_monitor(&monitors, args.monitor.as_deref());
            monitors = vec![monitors.swap_remove(pos)];
        }
        (start_capture(monitors.clone()), rect)
    };

    let wayland = monitors::is_wayland();
    let screens: Vec<app::Screen> = monitors
        .into_iter()
        .map(|monitor| app::Screen::new(viewport(&monitor, wayland), monitor, image::RgbaImage::new(0, 0)))
        .collect();

    let options = eframe::NativeOptions { viewport: screens[0].viewport.clone(), ..Default::default() };
    let result = eframe::run_native(
        "queijo",
        options,
        Box::new(|cc| {
            app::setup_style(&cc.egui_ctx);
            Ok(Box::new(app::Shared::new(app::QueijoApp::new(
                screens,
                cfg,
                (capture_job, desktop_rect),
                window_taken,
                shots_job,
            ))))
        }),
    );
    if let Err(e) = result {
        fail(&i18n::tf("error.open_editor", &[("error", &e.to_string())]));
    }
    std::process::exit(0);
}

/// Index of the monitor `--monitor` names (by name or number); exits if there is none.
fn pick_monitor(monitors: &[monitors::Monitor], wanted: Option<&str>) -> usize {
    let wanted = wanted.unwrap_or_default();
    monitors
        .iter()
        .position(|m| m.name == wanted || m.index.to_string() == wanted)
        .unwrap_or_else(|| fail(&i18n::tf("error.no_monitor", &[("name", wanted)])))
}

/// Turns a fresh capture into what the editor needs: one picture per shown
/// monitor, where the focused window is, and the job that grabs the desktop
/// again with the mouse pointer drawn in (see `QueijoApp::root_frame`).
fn prepare(shot: capture::Capture, all: &[monitors::Monitor], shown: &[monitors::Monitor]) -> app::Captured {
    let capture::Capture { desktop, shell, window } = shot;
    let window = window.and_then(|w| capture::locate_window(&desktop, &w, all));
    let pieces = capture::split(&desktop, all);
    let images = shown
        .iter()
        .map(|m| all.iter().position(|a| a.index == m.index).map(|i| pieces[i].clone()).unwrap_or_default())
        .collect();
    let (dimensions, all, shown) = (desktop.dimensions(), all.to_vec(), shown.to_vec());
    let pointer_job = shell.map(|shell| -> app::PointerJob {
        Box::new(move |grabbed| match shell.with_pointer(grabbed) {
            Some(img) if img.dimensions() == dimensions => {
                let pieces = capture::split(&img, &all);
                shown.iter().map(|m| all.iter().position(|a| a.index == m.index).map(|i| pieces[i].clone())).collect()
            }
            _ => vec![None; shown.len()],
        })
    });
    app::Captured { desktop: Arc::new(desktop), images, window, pointer_job }
}

/// `queijo --record x,y,w,h,pointer`, started by the editor: records until
/// another `queijo` (the shortcut pressed again) asks it to stop.
fn record_area(area: &str, cfg: &config::Config) -> ! {
    let Some(req) = record::Request::parse(area) else { fail(&i18n::tf("error.bad_area", &[("area", area)])) };
    // The editor that started us holds the instance socket until it has exited,
    // so getting it also means its windows are gone.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let listener = loop {
        if let Some(l) = single_instance() {
            break l;
        }
        if std::time::Instant::now() > deadline {
            fail(i18n::t("error.still_running"));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Give the compositor a moment to take the editor off screen.
    std::thread::sleep(Duration::from_millis(120));
    match record::run(req, &listener, cfg) {
        Ok(path) if req.gif => {
            let copied = std::fs::read(&path).is_ok_and(|data| output::copy_data("image/gif", &data).is_ok());
            let mb = std::fs::metadata(&path).map(|m| m.len() as f64 / 1e6).unwrap_or(0.0);
            let mut detail = format!("{} · {mb:.1} MB", output::tilde(&path));
            if copied {
                detail.push_str(&format!(" · {}", i18n::t("notify.gif_copied")));
            }
            output::notify(i18n::t("notify.gif_saved"), &detail, Some(&path));
            println!("{}", path.display());
            std::process::exit(0);
        }
        Ok(path) => {
            output::notify(i18n::t("notify.screencast_saved"), &output::tilde(&path), None);
            println!("{}", path.display());
            std::process::exit(0);
        }
        Err(e) => fail(&i18n::tf("error.recording_failed", &[("error", &e)])),
    }
}

fn viewport(monitor: &monitors::Monitor, wayland: bool) -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default()
        .with_title("Queijo")
        .with_app_id("queijo")
        .with_decorations(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_always_on_top()
        .with_active(true)
        .with_transparent(true)
        .with_inner_size([monitor.width as f32, monitor.height as f32]);
    if wayland {
        // Wayland hides absolute positions, so target the output directly.
        builder.with_monitor(monitor.index)
    } else {
        builder.with_position([monitor.x as f32, monitor.y as f32]).with_fullscreen(true)
    }
}

fn instance_addr() -> Option<SocketAddr> {
    let user = std::env::var("USER").unwrap_or_default();
    SocketAddr::from_abstract_name(format!("queijo-{user}")).ok()
}

fn single_instance() -> Option<UnixListener> {
    UnixListener::bind_addr(&instance_addr()?).ok()
}

/// Tells the running instance to stop recording.
fn signal_running() -> bool {
    let Some(addr) = instance_addr() else { return false };
    let Ok(mut stream) = UnixStream::connect_addr(&addr) else { return false };
    stream.write_all(record::STOP).is_ok()
}

fn fail(message: &str) -> ! {
    eprintln!("queijo: {message}");
    output::notify_error("Queijo", message);
    std::process::exit(1);
}
