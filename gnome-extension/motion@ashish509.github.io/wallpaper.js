import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import {InjectionManager} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Background from 'resource:///org/gnome/shell/ui/background.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as Workspace from 'resource:///org/gnome/shell/ui/workspace.js';
import * as WorkspaceThumbnail from 'resource:///org/gnome/shell/ui/workspaceThumbnail.js';

import {RENDERER_ID} from './constants.js';

const LiveWallpaper = GObject.registerClass(
class LiveWallpaper extends St.Widget {
    constructor(backgroundActor) {
        super({
            layout_manager: new Clutter.BinLayout(),
            x_expand: true,
            y_expand: true,
        });
        this._background = backgroundActor;
        this._clone = null;
        this._retryId = 0;
        this._sourceDestroyId = 0;
        backgroundActor.layout_manager = new Clutter.BinLayout();
        backgroundActor.add_child(this);
        this._allocationId = backgroundActor.connect('notify::allocation', () => this._resize());
        this.connect('destroy', () => this._dispose());
        this._attach(20);
    }

    _attach(attempts) {
        if (this._tryAttach())
            return;
        if (attempts === 0)
            return;
        this._retryId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 250, () => {
            if (this._tryAttach() || --attempts === 0) {
                this._retryId = 0;
                return GLib.SOURCE_REMOVE;
            }
            return GLib.SOURCE_CONTINUE;
        });
    }

    _tryAttach() {
        const monitor = this._background.monitor;
        const source = global.get_window_actors(false).find(actor =>
            actor.meta_window.title?.startsWith(`@${RENDERER_ID}|`) &&
            actor.meta_window.get_monitor() === monitor
        );
        if (!source)
            return false;

        this._clone = new Clutter.Clone({source});
        this.add_child(this._clone);
        this._sourceDestroyId = source.connect('destroy', () => {
            this._clone?.destroy();
            this._clone = null;
            this._sourceDestroyId = 0;
            this._attach(20);
        });
        this._resize();
        return true;
    }

    _resize() {
        const width = this._background.width;
        const height = this._background.height;
        this.set_size(width, height);
        this._clone?.set_size(width, height);
    }

    _dispose() {
        if (this._retryId)
            GLib.source_remove(this._retryId);
        if (this._allocationId)
            this._background?.disconnect(this._allocationId);
        if (this._sourceDestroyId && this._clone?.source)
            this._clone.source.disconnect(this._sourceDestroyId);
        this._background = null;
        this._clone = null;
    }
});

export class WallpaperIntegration {
    constructor() {
        this._injections = new InjectionManager();
        this._managedWindows = new Map();
    }

    enable(client) {
        this._client = client;
        const integration = this;

        this._injections.overrideMethod(
            Background.BackgroundManager.prototype,
            '_createBackgroundActor',
            original => function () {
                const actor = original.call(this);
                this.motionWallpaper = new LiveWallpaper(actor);
                return actor;
            }
        );
        this._injections.overrideMethod(
            Shell.Global.prototype,
            'get_window_actors',
            original => function (hideRenderer = true) {
                const actors = original.call(this);
                return hideRenderer
                    ? actors.filter(actor => !isRenderer(actor.meta_window))
                    : actors;
            }
        );
        this._injections.overrideMethod(
            Workspace.Workspace.prototype,
            '_isOverviewWindow',
            original => function (window) {
                return !isRenderer(window) && original.call(this, window);
            }
        );
        this._injections.overrideMethod(
            WorkspaceThumbnail.WorkspaceThumbnail.prototype,
            '_isOverviewWindow',
            original => function (window) {
                return !isRenderer(window) && original.call(this, window);
            }
        );
        this._injections.overrideMethod(
            Meta.Display.prototype,
            'get_tab_list',
            original => function (type, workspace) {
                return original.call(this, type, workspace).filter(window => !isRenderer(window));
            }
        );
        this._injections.overrideMethod(
            Shell.WindowTracker.prototype,
            'get_window_app',
            original => function (window) {
                return isRenderer(window) ? null : original.call(this, window);
            }
        );

        this._mapId = global.window_manager.connect_after('map', (_manager, actor) => {
            const window = actor.get_meta_window();
            if (integration._client?.owns(window))
                integration._manage(window);
        });
        Main.layoutManager._updateBackgrounds();
    }

    setClient(client) {
        this._client = client;
    }

    _manage(window) {
        if (this._managedWindows.has(window))
            return;
        const keepHidden = () => {
            if (!window.minimized)
                window.minimize();
            if (window.above)
                window.unmake_above();
            window.lower();
        };
        const signals = [
            window.connect_after('raised', keepHidden),
            window.connect('notify::minimized', keepHidden),
            window.connect('notify::above', keepHidden),
        ];
        signals.push(window.connect('unmanaged', () => this._unmanage(window)));
        this._managedWindows.set(window, signals);
        window.stick();
        keepHidden();
    }

    _unmanage(window) {
        const signals = this._managedWindows.get(window) ?? [];
        for (const signal of signals) {
            try {
                window.disconnect(signal);
            } catch {
                continue;
            }
        }
        this._managedWindows.delete(window);
    }

    disable() {
        if (this._mapId)
            global.window_manager.disconnect(this._mapId);
        for (const window of [...this._managedWindows.keys()])
            this._unmanage(window);
        this._injections.clear();
        this._client = null;
        Main.layoutManager._updateBackgrounds();
    }
}

function isRenderer(window) {
    return window?.title?.startsWith(`@${RENDERER_ID}|`) ?? false;
}