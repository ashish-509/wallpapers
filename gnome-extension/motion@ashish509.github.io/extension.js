import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {RENDERER_ID, RENDERER_OBJECT_PATH} from './constants.js';
import {PausePolicy} from './policy.js';
import {RendererClient} from './renderer.js';
import {WallpaperIntegration} from './wallpaper.js';

export default class MotionExtension extends Extension {
    enable() {
        this._enabled = true;
        this._settings = this.getSettings();
        this._integration = new WallpaperIntegration();
        this._integration.enable(null);
        this._policy = new PausePolicy(this._settings, paused => {
            this._paused = paused;
            this._syncPlayback();
        });
        this._policy.enable();
        this._activeId = this._settings.connect('changed::active-file', () => this._updateFile());
        this._commandId = this._settings.connect('changed::renderer-command', () => this._restart());
        this._enabledId = this._settings.connect('changed::enabled', () => this._setEnabled());
        this._monitorsId = Main.layoutManager.connect('monitors-changed', () => this._restart());
        this._launch();
    }

    _launch() {
        if (!this._settings.get_boolean('enabled'))
            return;
        const file = this._settings.get_string('active-file');
        const command = this._settings.get_strv('renderer-command');
        if (!file || command.length === 0 || !Gio.File.new_for_path(file).query_exists(null))
            return;

        const client = new RendererClient(() => {
            if (!this._enabled || client !== this._client)
                return;
            this._client = null;
            this._integration.setClient(null);
            this._restartId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 2, () => {
                this._restartId = 0;
                this._launch();
                return GLib.SOURCE_REMOVE;
            });
        });
        this._client = client;
        this._integration.setClient(client);
        try {
            client.start(command, file);
        } catch (error) {
            console.error(`Motion renderer failed to start: ${error.message}`);
            this._client = null;
            this._integration.setClient(null);
        }
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 500, () => {
            this._syncPlayback();
            return GLib.SOURCE_REMOVE;
        });
    }

    _syncPlayback() {
        const actions = Gio.DBusActionGroup.get(
            Gio.DBus.session,
            RENDERER_ID,
            RENDERER_OBJECT_PATH
        );
        actions.activate_action(this._paused ? 'pause' : 'play', null);
    }

    _updateFile() {
        if (!this._settings.get_boolean('enabled'))
            return;
        const file = this._settings.get_string('active-file');
        if (!file || !Gio.File.new_for_path(file).query_exists(null))
            return;
        // Swap the file in the running renderer instead of relaunching it, so the
        // static desktop wallpaper never flashes between wallpapers.
        if (this._client) {
            Gio.DBusActionGroup.get(
                Gio.DBus.session,
                RENDERER_ID,
                RENDERER_OBJECT_PATH
            ).activate_action('set-file', new GLib.Variant('s', file));
            GLib.timeout_add(GLib.PRIORITY_DEFAULT, 200, () => {
                this._syncPlayback();
                return GLib.SOURCE_REMOVE;
            });
        } else {
            this._launch();
        }
    }

    _setEnabled() {
        if (this._settings.get_boolean('enabled')) {
            if (!this._client)
                this._launch();
        } else {
            this._stop();
        }
    }

    _stop() {
        if (this._restartId) {
            GLib.source_remove(this._restartId);
            this._restartId = 0;
        }
        this._client?.stop();
        this._client = null;
        this._integration.setClient(null);
    }

    _restart() {
        if (this._restartId) {
            GLib.source_remove(this._restartId);
            this._restartId = 0;
        }
        this._client?.stop();
        this._client = null;
        this._integration.setClient(null);
        this._launch();
    }

    disable() {
        this._enabled = false;
        if (this._restartId)
            GLib.source_remove(this._restartId);
        if (this._activeId)
            this._settings.disconnect(this._activeId);
        if (this._commandId)
            this._settings.disconnect(this._commandId);
        if (this._enabledId)
            this._settings.disconnect(this._enabledId);
        if (this._monitorsId)
            Main.layoutManager.disconnect(this._monitorsId);
        this._policy.disable();
        this._integration.disable();
        this._client?.stop();
        this._policy = null;
        this._integration = null;
        this._client = null;
        this._settings = null;
    }
}