// Runs the GNOME 45+ extension under Node with stand-ins for GNOME's modules.
//   node tests/extension/run.mjs gnome-extension/esm/queijo-gnome45@queijo.app/extension.js
import assert from 'node:assert/strict';
import {register} from 'node:module';
import path from 'node:path';
import {pathToFileURL} from 'node:url';

register('./hooks.mjs', import.meta.url);

const file = process.argv[2];
assert.ok(file, 'usage: run.mjs EXTENSION.js');

// What the Shell gives an extension as globals.
const windows = [];
globalThis.global = {
  workspace_manager: {get_active_workspace: () => 'workspace-1'},
  get_window_actors: () => windows.map(w => w.actor),
};
globalThis.logError = (e, what) => (globalThis.__errors = globalThis.__errors ?? []).push(`${what}: ${e.message}`);

const rect = (x, y, width, height) => ({x, y, width, height});
function fake(id, title, app, wmClass, frame, {minimized = false, skipTaskbar = false, type = 0, workspace = 'workspace-1', focus = false, image = null} = {}) {
  const w = {
    app, minimized, skip_taskbar: skipTaskbar,
    get_id: () => id, get_title: () => title, get_wm_class: () => wmClass, get_window_type: () => type,
    located_on_workspace: ws => ws === workspace, has_focus: () => focus,
    get_frame_rect: () => rect(...frame), get_buffer_rect: () => rect(frame[0] - 10, frame[1] - 10, frame[2] + 20, frame[3] + 20),
  };
  const actor = {get_meta_window: () => w};
  if (image) actor.get_image = () => image;
  w.actor = actor;
  return w;
}

const {default: Extension} = await import(pathToFileURL(path.resolve(file)));
const extension = new Extension();
extension.enable();

// The D-Bus interface the program talks to.
const {xml, service} = globalThis.__exported;
const methods = Object.fromEntries(
  [...xml.matchAll(/<method name="(\w+)">([\s\S]*?)<\/method>/g)].map(([, name, body]) => [
    name,
    [...body.matchAll(/<arg type="([^"]+)" direction="(in|out)"/g)].map(([, type, dir]) => `${dir}:${type}`),
  ]),
);
assert.deepEqual(methods.ListWindows, ['out:a(tsssiiiiiiiib)']);
assert.deepEqual(methods.CaptureWindow, ['in:t', 'in:s', 'out:b']);
assert.deepEqual(methods.Screenshot, ['in:s', 'in:b', 'out:b']);
assert.deepEqual(globalThis.__log.find(l => l[0] === 'export'), ['export', '/app/queijo/Queijo']);
assert.equal(globalThis.__watch.name, 'app.queijo.Queijo.Recorder');

// ListWindows: topmost first, and only what a person would want a picture of.
windows.push(
  fake(1, 'bottom', 'Files', 'org.gnome.Files', [0, 0, 400, 300]),
  fake(2, 'minimized', 'Hidden', 'hidden', [0, 0, 10, 10], {minimized: true}),
  fake(3, 'panel', 'Dock', 'dock', [0, 0, 10, 10], {skipTaskbar: true}),
  fake(4, 'popup', 'Menu', 'menu', [0, 0, 10, 10], {type: 5}),
  fake(5, 'elsewhere', 'Away', 'away', [0, 0, 10, 10], {workspace: 'workspace-2'}),
  fake(6, 'Queijo', 'Queijo', 'queijo', [0, 0, 1280, 720]),
  fake(7, 'top', '', 'Gedit', [100, 50, 300, 200], {focus: true}),
);
const listed = service.ListWindows();
assert.deepEqual(listed.map(r => r[1]), ['top', 'bottom']);
assert.deepEqual(listed[0], [7, 'top', '', 'Gedit', 100, 50, 300, 200, 90, 40, 320, 220, true]);
assert.equal(listed[1][2], 'Files', 'the app name comes from the window tracker');
assert.ok(listed.every(r => r.length === 13));

// CaptureWindow: false when the Shell can't give the window's pixels, true when it can.
assert.equal(service.CaptureWindow(1, '/tmp/x.png'), false);
let written = null;
windows.push(fake(8, 'imaged', 'A', 'a', [0, 0, 1, 1], {image: {writeToPNG: p => (written = p)}}));
assert.equal(service.CaptureWindow(8, '/tmp/win.png'), true);
assert.equal(written, '/tmp/win.png');
assert.equal(service.CaptureWindow(99, '/tmp/none.png'), false, 'no such window');

// Screenshot: the Shell's own screenshot object writes into a file we opened.
const answers = [];
const invocation = {return_value: v => answers.push(v)};
await service.ScreenshotAsync(['/run/user/1/shot.png', true], invocation);
const plain = v => ({type: v.type, value: v.value});
assert.deepEqual(plain(answers.pop()), {type: '(b)', value: [true]});
assert.equal(globalThis.__shots[0].includeCursor, true);
assert.equal(globalThis.__streams[0].path, '/run/user/1/shot.png');
assert.ok(globalThis.__streams[0].closed, 'the file is closed afterwards');
globalThis.__screenshotFails = true;
await service.ScreenshotAsync(['/run/user/1/fail.png', false], invocation);
assert.deepEqual(plain(answers.pop()), {type: '(b)', value: [false]}, 'a refusal is an answer, not a crash');
assert.ok(globalThis.__streams[1].closed, 'and the file is still closed');
assert.equal(globalThis.__errors.length, 1);

// The recording indicator shows while the recorder owns its name.
globalThis.__watch.appeared();
assert.equal(globalThis.__panel.length, 1);
assert.equal(globalThis.__panel[0].name, 'queijo-recording');
globalThis.__watch.appeared();
assert.equal(globalThis.__panel.length, 1, 'only one indicator');
globalThis.__panel[0].button.handlers['button-press-event']();
assert.deepEqual(globalThis.__log.find(l => l[0] === 'dbus-call'), ['dbus-call', 'app.queijo.Queijo.Recorder', 'Stop']);
globalThis.__watch.vanished();
assert.ok(globalThis.__panel[0].button.destroyed);
assert.ok(globalThis.__log.some(l => l[0] === 'source_remove' && l[1] === 7), 'its timer stops');

extension.disable();
assert.ok(globalThis.__log.some(l => l[0] === 'unexport'));
assert.ok(globalThis.__log.some(l => l[0] === 'unwatch'));
console.log('ok:', path.basename(path.dirname(file)));
