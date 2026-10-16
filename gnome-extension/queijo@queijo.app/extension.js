/* Queijo helper.
 *
 * GNOME doesn't let other apps see which windows exist or where they are, so
 * the Queijo screenshot tool asks this extension (which runs inside the Shell)
 * for the window list and for pictures of single windows. It also shows a
 * recording indicator in the top bar while `queijo --record` runs.
 */
const { Clutter, Gio, GLib, GObject, Meta, Shell, St } = imports.gi;
const Main = imports.ui.main;
const PanelMenu = imports.ui.panelMenu;

const OBJECT_PATH = '/app/queijo/Queijo';
const IFACE = `<node>
  <interface name="app.queijo.Queijo">
    <method name="ListWindows">
      <arg type="a(tsssiiiiiiiib)" direction="out" name="windows"/>
    </method>
    <method name="CaptureWindow">
      <arg type="t" direction="in" name="id"/>
      <arg type="s" direction="in" name="path"/>
      <arg type="b" direction="out" name="ok"/>
    </method>
  </interface>
</node>`;

const RECORDER_NAME = 'app.queijo.Queijo.Recorder';
const RECORDER_PATH = '/app/queijo/Queijo/Recorder';

const PICKABLE = [Meta.WindowType.NORMAL, Meta.WindowType.DIALOG, Meta.WindowType.MODAL_DIALOG];

class Service {
    /* Windows on the active workspace, topmost first:
     * (id, title, app name, wm class, frame x/y/w/h, buffer x/y/w/h, focused) */
    ListWindows() {
        const workspace = global.workspace_manager.get_active_workspace();
        const tracker = Shell.WindowTracker.get_default();
        return global.get_window_actors()
            .map(actor => actor.get_meta_window())
            .filter(w => w && !w.minimized && !w.skip_taskbar &&
                PICKABLE.includes(w.get_window_type()) &&
                w.located_on_workspace(workspace) &&
                w.get_wm_class() !== 'queijo')
            .reverse()
            .map(w => {
                const f = w.get_frame_rect();
                const b = w.get_buffer_rect();
                const app = tracker.get_window_app(w);
                return [
                    w.get_id(), w.get_title() || '', app ? app.get_name() : '', w.get_wm_class() || '',
                    f.x, f.y, f.width, f.height, b.x, b.y, b.width, b.height,
                    w.has_focus(),
                ];
            });
    }

    /* Writes the window's own pixels (whole buffer, shadows included) as PNG,
     * even if other windows cover it. */
    CaptureWindow(id, path) {
        const actor = global.get_window_actors().find(a => a.get_meta_window()?.get_id() === id);
        const surface = actor?.get_image(null);
        if (!surface)
            return false;
        surface.writeToPNG(path);
        return true;
    }
}

const RecordingIndicator = GObject.registerClass(
class QueijoRecordingIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.0, 'Queijo recording', true);

        const box = new St.BoxLayout({ style_class: 'panel-status-menu-box' });
        this._dot = new St.Icon({
            icon_name: 'media-record-symbolic',
            style_class: 'system-status-icon',
            style: 'color: #ff453a;',
        });
        this._time = new St.Label({ text: '0:00', y_align: Clutter.ActorAlign.CENTER });
        const stop = new St.Icon({
            icon_name: 'media-playback-stop-symbolic',
            style_class: 'system-status-icon',
            style: 'margin-left: 6px;',
        });
        box.add_child(this._dot);
        box.add_child(this._time);
        box.add_child(stop);
        this.add_child(box);

        this._started = GLib.get_monotonic_time();
        this._tick = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 500, () => {
            this._update();
            return GLib.SOURCE_CONTINUE;
        });
        this.connect('button-press-event', () => this._stop());
        this.connect('touch-event', (_actor, event) => {
            if (event.type() === Clutter.EventType.TOUCH_BEGIN)
                return this._stop();
            return Clutter.EVENT_PROPAGATE;
        });
    }

    _update() {
        const secs = Math.floor((GLib.get_monotonic_time() - this._started) / 1e6);
        this._time.text = `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, '0')}`;
        // Gentle pulse, like a camera's tally light.
        this._dot.ease({
            opacity: this._dot.opacity > 200 ? 110 : 255,
            duration: 450,
            mode: Clutter.AnimationMode.EASE_IN_OUT_QUAD,
        });
    }

    _stop() {
        Gio.DBus.session.call(RECORDER_NAME, RECORDER_PATH, RECORDER_NAME, 'Stop',
            null, null, Gio.DBusCallFlags.NONE, -1, null, null);
        return Clutter.EVENT_STOP;
    }

    _onDestroy() {
        GLib.source_remove(this._tick);
        super._onDestroy();
    }
});

class Extension {
    enable() {
        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE, new Service());
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);

        // The recorder owns its bus name exactly while it is recording.
        this._watch = Gio.bus_watch_name(Gio.BusType.SESSION, RECORDER_NAME, Gio.BusNameWatcherFlags.NONE,
            () => this._showIndicator(), () => this._hideIndicator());
    }

    disable() {
        Gio.bus_unwatch_name(this._watch);
        this._hideIndicator();
        this._dbus.unexport();
        this._dbus = null;
    }

    _showIndicator() {
        if (!this._indicator) {
            this._indicator = new RecordingIndicator();
            Main.panel.addToStatusArea('queijo-recording', this._indicator, 0, 'right');
        }
    }

    _hideIndicator() {
        this._indicator?.destroy();
        this._indicator = null;
    }
}

function init() {
    return new Extension();
}
