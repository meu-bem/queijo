use std::process::Command;

/// A physical output, in compositor (logical) coordinates.
#[derive(Clone, Debug, Default)]
pub struct Monitor {
    /// Position of this output in the windowing backend's monitor list
    /// (the index `ViewportBuilder::with_monitor` expects).
    pub index: usize,
    pub name: String,
    pub description: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Monitor {
    pub fn label(&self) -> String {
        if self.description.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.description)
        }
    }
}

pub fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

pub fn detect() -> Vec<Monitor> {
    if is_wayland() {
        match wayland::outputs() {
            Ok(outputs) if !outputs.is_empty() => return outputs,
            Ok(_) => eprintln!("queijo: compositor reported no outputs"),
            Err(e) => eprintln!("queijo: could not list Wayland outputs: {e}"),
        }
    }
    xrandr()
}

/// Parses `xrandr --listmonitors`, e.g. ` 0: +*HDMI-1 2560/800x1080/340+0+0  HDMI-1`.
fn xrandr() -> Vec<Monitor> {
    let Ok(out) = Command::new("xrandr").arg("--listmonitors").output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .enumerate()
        .filter_map(|(index, line)| {
            let mut parts = line.split_whitespace();
            let _idx = parts.next()?;
            let name = parts.next()?.trim_start_matches(['+', '*']).to_string();
            let geom = parts.next()?;
            let (w, rest) = geom.split_once('/')?;
            let (_, rest) = rest.split_once('x')?;
            let (h, rest) = rest.split_once('/')?;
            let mut offs = rest.splitn(3, '+').skip(1);
            Some(Monitor {
                index,
                name,
                description: String::new(),
                x: offs.next()?.parse().ok()?,
                y: offs.next()?.parse().ok()?,
                width: w.parse().ok()?,
                height: h.parse().ok()?,
            })
        })
        .collect()
}

mod wayland {
    use super::Monitor;
    use wayland_client::{
        globals::{registry_queue_init, GlobalListContents},
        protocol::{
            wl_output::{self, WlOutput},
            wl_registry::{self, WlRegistry},
        },
        Connection, Dispatch, QueueHandle, WEnum,
    };
    use wayland_protocols::xdg::xdg_output::zv1::client::{
        zxdg_output_manager_v1::{self, ZxdgOutputManagerV1},
        zxdg_output_v1::{self, ZxdgOutputV1},
    };

    struct State {
        outputs: Vec<Monitor>,
    }

    impl Dispatch<WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<WlOutput, usize> for State {
        fn event(
            state: &mut Self,
            _: &WlOutput,
            event: wl_output::Event,
            idx: &usize,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let m = &mut state.outputs[*idx];
            match event {
                wl_output::Event::Geometry { x, y, .. } => {
                    m.x = x;
                    m.y = y;
                }
                wl_output::Event::Mode { flags: WEnum::Value(flags), width, height, .. }
                    if flags.contains(wl_output::Mode::Current) =>
                {
                    m.width = width;
                    m.height = height;
                }
                wl_output::Event::Name { name } => m.name = name,
                wl_output::Event::Description { description } => m.description = description,
                _ => {}
            }
        }
    }

    impl Dispatch<ZxdgOutputManagerV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &ZxdgOutputManagerV1,
            _: zxdg_output_manager_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZxdgOutputV1, usize> for State {
        fn event(
            state: &mut Self,
            _: &ZxdgOutputV1,
            event: zxdg_output_v1::Event,
            idx: &usize,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let m = &mut state.outputs[*idx];
            match event {
                zxdg_output_v1::Event::LogicalPosition { x, y } => {
                    m.x = x;
                    m.y = y;
                }
                zxdg_output_v1::Event::LogicalSize { width, height } => {
                    m.width = width;
                    m.height = height;
                }
                zxdg_output_v1::Event::Name { name } if m.name.is_empty() => m.name = name,
                zxdg_output_v1::Event::Description { description } if m.description.is_empty() => {
                    m.description = description
                }
                _ => {}
            }
        }
    }

    /// Lists outputs in registry order, which is the same order winit uses
    /// for `available_monitors()`.
    pub fn outputs() -> Result<Vec<Monitor>, Box<dyn std::error::Error>> {
        let conn = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
        let qh = queue.handle();
        let mut state = State { outputs: Vec::new() };

        let mut wl_outputs = Vec::new();
        for global in globals.contents().clone_list() {
            if global.interface != "wl_output" {
                continue;
            }
            let index = state.outputs.len();
            state.outputs.push(Monitor { index, ..Default::default() });
            let output: WlOutput = globals.registry().bind(global.name, global.version.min(4), &qh, index);
            wl_outputs.push(output);
        }
        queue.roundtrip(&mut state)?;

        if let Ok(manager) = globals.bind::<ZxdgOutputManagerV1, _, _>(&qh, 1..=3, ()) {
            for (index, output) in wl_outputs.iter().enumerate() {
                manager.get_xdg_output(output, &qh, index);
            }
            queue.roundtrip(&mut state)?;
        }

        state.outputs.retain(|m| m.width > 0 && m.height > 0);
        Ok(state.outputs)
    }
}
