// Loader hooks that stand in for the modules GNOME Shell gives an extension
// (`gi://…` and `resource:///…`), so its code can run under plain Node.
// What each stand-in does is only as much as the extension asks of it.

const stubs = {
  'gi://Clutter': `export default {
    ActorAlign: {CENTER: 1}, EVENT_PROPAGATE: false, EVENT_STOP: true,
    EventType: {TOUCH_BEGIN: 1}, AnimationMode: {EASE_IN_OUT_QUAD: 1},
  };`,
  'gi://Gio': `
    const log = globalThis.__log = globalThis.__log ?? [];
    export default {
      _promisify() {},
      DBusExportedObject: {
        wrapJSObject(xml, service) {
          globalThis.__exported = {xml, service};
          return {export(bus, path) { log.push(['export', path]); }, unexport() { log.push(['unexport']); }};
        },
      },
      DBus: {session: {call(...args) { log.push(['dbus-call', args[0], args[3]]); }}},
      DBusCallFlags: {NONE: 0},
      BusType: {SESSION: 1}, BusNameWatcherFlags: {NONE: 0},
      bus_watch_name(type, name, flags, appeared, vanished) {
        globalThis.__watch = {name, appeared, vanished};
        return 42;
      },
      bus_unwatch_name(id) { log.push(['unwatch', id]); },
      File: {
        new_for_path(path) {
          return {replace(...args) {
            const stream = {path, closed: false, close() { this.closed = true; }};
            (globalThis.__streams = globalThis.__streams ?? []).push(stream);
            return stream;
          }};
        },
      },
      FileCreateFlags: {NONE: 0},
    };`,
  'gi://GLib': `
    export default {
      Variant: class { constructor(type, value) { this.type = type; this.value = value; } },
      timeout_add() { return 7; }, source_remove(id) { (globalThis.__log ??= []).push(['source_remove', id]); },
      SOURCE_CONTINUE: true, PRIORITY_DEFAULT: 0, get_monotonic_time() { return 0; },
    };`,
  'gi://GObject': `export default {
    registerClass(cls) { return class extends cls { constructor(...args) { super(); this._init(...args); } }; },
  };`,
  'gi://Meta': `export default {WindowType: {NORMAL: 0, DIALOG: 3, MODAL_DIALOG: 4, DOCK: 5}};`,
  'gi://Shell': `
    class Screenshot {
      async screenshot(includeCursor, stream) {
        (globalThis.__shots = globalThis.__shots ?? []).push({includeCursor, stream});
        if (globalThis.__screenshotFails) throw new Error('refused');
      }
    }
    export default {
      Screenshot,
      WindowTracker: {get_default() { return {get_window_app: w => w.app ? {get_name: () => w.app} : null}; }},
    };`,
  'gi://St': `
    class Widget { add_child(c) { (this.children ??= []).push(c); } }
    export default {BoxLayout: class extends Widget {}, Icon: class extends Widget {}, Label: class extends Widget {}};`,
  'resource:///org/gnome/shell/extensions/extension.js': `export class Extension {}`,
  'resource:///org/gnome/shell/ui/main.js': `
    export const panel = {addToStatusArea(name, button, position, box) {
      (globalThis.__panel = globalThis.__panel ?? []).push({name, button, position, box});
    }};`,
  'resource:///org/gnome/shell/ui/panelMenu.js': `
    export class Button {
      _init() { this.handlers = {}; }
      connect(signal, handler) { this.handlers[signal] = handler; }
      add_child() {}
      destroy() { this.destroyed = true; this.handlers.destroy?.(); }
    }`,
};

export async function resolve(specifier, context, next) {
  if (specifier in stubs) return {url: `queijo-stub:${specifier}`, shortCircuit: true};
  return next(specifier, context);
}

export async function load(url, context, next) {
  if (url.startsWith('queijo-stub:')) {
    return {format: 'module', source: stubs[url.slice('queijo-stub:'.length)], shortCircuit: true};
  }
  return next(url, context);
}
