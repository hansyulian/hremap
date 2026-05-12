import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const IFACE = `
<node>
  <interface name="org.gnome.shell.extensions.FocusedWindow">
    <method name="Get">
      <arg type="s" direction="out" name="window"/>
    </method>
    <signal name="FocusChanged">
      <arg type="s" name="window"/>
    </signal>
  </interface>
</node>`;

const defaultWindowInfoJSON = JSON.stringify({
    title: '',
    wm_class: '',
    wm_class_instance: '',
    pid: 0,
});

function getWindowInfo(win) {
    if (!win) return defaultWindowInfoJSON;
    try {
        return JSON.stringify({
            title: win.get_title() ?? '',
            wm_class: win.get_wm_class() ?? '',
            wm_class_instance: win.get_wm_class_instance() ?? '',
            pid: win.get_pid(),
        });
    } catch {
        return defaultWindowInfoJSON;
    }
    return defaultWindowInfoJSON;
}

class FocusedWindowDBus {
    constructor() {
        this._dbusImpl = Gio.DBusExportedObject.wrapJSObject(IFACE, this);
        this._dbusImpl.export(Gio.DBus.session, '/org/gnome/shell/extensions/FocusedWindow');
        this._focusConn = global.display.connect('notify::focus-window', () => this._onFocusChanged());
    }

    Get() {
        const win = global.display.focus_window;
        return getWindowInfo(win) ?? '{}';
    }

    _onFocusChanged() {
        const win = global.display.focus_window;
        const json = getWindowInfo(win);
        this._dbusImpl.emit_signal('FocusChanged', new GLib.Variant('(s)', [json]));
    }

    destroy() {
        global.display.disconnect(this._focusConn);
        this._dbusImpl.unexport();
    }
}

let dbus = null;

export default class Extension {
    enable() {
        dbus = new FocusedWindowDBus();
    }

    disable() {
        dbus?.destroy();
        dbus = null;
    }
}
