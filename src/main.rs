mod app;
mod capture;
mod cli;
mod config;
mod monitors;
mod output;
mod gif;
mod icon;
mod paint;
mod record;
mod settings;
mod shell_ext;
mod shortcut;

use std::os::linux::net::SocketAddrExt;
use std::io::Write;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use eframe::egui;

fn main() {
    let args = cli::Cli::parse();

    if args.settings {
        if let Err(e) = settings::run() {
            fail(&format!("Could not open the settings: {e}"));
        }
        return;
    }

    let mut cfg = config::Config::load();
    output::set_notifications(cfg.notifications);
    if let Some(area) = &args.record {
        record_area(area, &cfg);
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

    let (desktop, shell) = match capture::capture() {
        Ok(c) => c,
        Err(e) => fail(&format!("Could not capture the screen: {e}")),
    };

    if monitors.is_empty() {
        monitors.push(monitors::Monitor {
            name: "screen".into(),
            width: desktop.width() as i32,
            height: desktop.height() as i32,
            ..Default::default()
        });
    }
    let mut images = capture::split(&desktop, &monitors);
    let all_monitors = monitors.clone();

    if let Some(wanted) = &args.monitor {
        let Some(pos) = monitors.iter().position(|m| m.name == *wanted || m.index.to_string() == *wanted) else {
            fail(&format!("No monitor named '{wanted}' (see --list-monitors)"));
        };
        monitors = vec![monitors.swap_remove(pos)];
        images = vec![images.swap_remove(pos)];
    }

    if args.quick {
        let image = if args.monitor.is_some() { &images[0] } else { &desktop };
        match output::save_image(image, &cfg.screenshots, cfg.image_format, cfg.jpeg_quality) {
            Ok(path) => println!("{}", path.display()),
            Err(e) => fail(&format!("Could not save the screenshot: {e}")),
        }
        return;
    }

    // Locate the focused window while the editor window starts up...
    let desktop = Arc::new(desktop);
    let desktop_rect = {
        let min_x = all_monitors.iter().map(|m| m.x).min().unwrap_or(0) as f32;
        let min_y = all_monitors.iter().map(|m| m.y).min().unwrap_or(0) as f32;
        let max_x = all_monitors.iter().map(|m| m.x + m.width).max().unwrap_or(desktop.width() as i32) as f32;
        let max_y = all_monitors.iter().map(|m| m.y + m.height).max().unwrap_or(desktop.height() as i32) as f32;
        egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y))
    };
    let desktop_for_app = desktop.clone();

    // With the Queijo Shell extension we can offer every window; otherwise only
    // the focused one, found by matching its picture on the screen.
    let extension = zbus::blocking::Connection::session()
        .ok()
        .and_then(|conn| shell_ext::list_windows(&conn).map(|list| (conn, list)));
    let shots_job = extension.map(|(conn, list)| std::thread::spawn(move || shell_ext::capture_all(&conn, list)));
    let window_taken = Arc::new(AtomicBool::new(shell.is_none() || shots_job.is_some()));
    let window_job = shell.clone().filter(|_| shots_job.is_none()).map(|shell| {
        let (desktop, all, taken) = (desktop.clone(), all_monitors.clone(), window_taken.clone());
        std::thread::spawn(move || shell.window(&taken).and_then(|w| capture::locate_window(&desktop, &w, &all)))
    });
    // ...and grab the pointer variant once it is up (see `QueijoApp::ui`).
    let shown = monitors.clone();
    let pointer_job = shell.map(|shell| -> app::PointerJob {
        Box::new(move |grabbed| match shell.with_pointer(grabbed) {
            Some(img) if img.dimensions() == desktop.dimensions() => {
                let all = capture::split(&img, &all_monitors);
                shown.iter().map(|m| all_monitors.iter().position(|a| a.index == m.index).map(|i| all[i].clone())).collect()
            }
            _ => vec![None; shown.len()],
        })
    });

    let wayland = monitors::is_wayland();
    let screens: Vec<app::Screen> = monitors
        .into_iter()
        .zip(images)
        .map(|(monitor, image)| app::Screen::new(viewport(&monitor, wayland), monitor, image))
        .collect();

    let options = eframe::NativeOptions {
        viewport: screens[0].viewport.clone(),
        ..Default::default()
    };
    let result = eframe::run_native(
        "queijo",
        options,
        Box::new(|cc| {
            app::setup_style(&cc.egui_ctx);
            Ok(Box::new(app::Shared::new(app::QueijoApp::new(
                screens,
                cfg,
                (desktop_for_app, desktop_rect),
                (window_job, window_taken),
                shots_job,
                pointer_job,
            ))))
        }),
    );
    if let Err(e) = result {
        fail(&format!("Could not open the editor: {e}"));
    }
    std::process::exit(0);
}

/// `queijo --record x,y,w,h,pointer`, started by the editor: records until
/// another `queijo` (the shortcut pressed again) asks it to stop.
fn record_area(area: &str, cfg: &config::Config) -> ! {
    let Some(req) = record::Request::parse(area) else { fail(&format!("Bad recording area '{area}'")) };
    // The editor that started us holds the instance socket until it has exited,
    // so getting it also means its windows are gone.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let listener = loop {
        if let Some(l) = single_instance() {
            break l;
        }
        if std::time::Instant::now() > deadline {
            fail("Another Queijo is still running; recording cancelled");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Give the compositor a moment to take the editor off screen.
    std::thread::sleep(Duration::from_millis(120));
    match record::run(req, &listener, cfg) {
        Ok(path) if req.gif => {
            let copied = std::fs::read(&path).is_ok_and(|data| output::copy_data("image/gif", &data).is_ok());
            let mb = std::fs::metadata(&path).map(|m| m.len() as f64 / 1e6).unwrap_or(0.0);
            let detail = format!(
                "{} · {mb:.1} MB{}",
                output::tilde(&path),
                if copied { " · copied to clipboard" } else { "" }
            );
            output::notify("GIF saved", &detail, Some(&path));
            println!("{}", path.display());
            std::process::exit(0);
        }
        Ok(path) => {
            output::notify("Screencast saved", &output::tilde(&path), None);
            println!("{}", path.display());
            std::process::exit(0);
        }
        Err(e) => fail(&format!("Recording failed: {e}")),
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
