import Gio from 'gi://Gio';

import {RENDERER_ID} from './constants.js';

export class PausePolicy {
    constructor(settings, changed) {
        this._settings = settings;
        this._changed = changed;
        this._paused = null;
        this._windows = new Map();
        this._signals = [];
        this._proxies = [];
    }

    enable() {
        this._workspaceManager = global.workspace_manager;
        this._signals.push([
            this._workspaceManager,
            this._workspaceManager.connect('active-workspace-changed', () => this._bindWorkspace()),
        ]);
        this._bindWorkspace();

        this._gameMode = this._proxy(
            Gio.BusType.SESSION,
            'com.feralinteractive.GameMode',
            '/com/feralinteractive/GameMode',
            'com.feralinteractive.GameMode'
        );
        this._powerProfile = this._proxy(
            Gio.BusType.SESSION,
            'org.freedesktop.portal.Desktop',
            '/org/freedesktop/portal/desktop',
            'org.freedesktop.portal.PowerProfileMonitor'
        );
        this._upower = this._proxy(
            Gio.BusType.SYSTEM,
            'org.freedesktop.UPower',
            '/org/freedesktop/UPower',
            'org.freedesktop.UPower'
        );
        this._battery = this._proxy(
            Gio.BusType.SYSTEM,
            'org.freedesktop.UPower',
            '/org/freedesktop/UPower/devices/DisplayDevice',
            'org.freedesktop.UPower.Device'
        );
        this._settingsId = this._settings.connect('changed', () => this.evaluate());
        this.evaluate();
    }

    _proxy(bus, name, path, interfaceName) {
        try {
            const proxy = Gio.DBusProxy.new_for_bus_sync(
                bus,
                Gio.DBusProxyFlags.DO_NOT_AUTO_START,
                null,
                name,
                path,
                interfaceName,
                null
            );
            const signal = proxy.connect('g-properties-changed', () => this.evaluate());
            this._proxies.push([proxy, signal]);
            return proxy;
        } catch {
            return null;
        }
    }

    _bindWorkspace() {
        this._unbindWorkspace();
        this._workspace = this._workspaceManager.get_active_workspace();
        this._workspaceAddedId = this._workspace.connect('window-added', (_workspace, window) => {
            this._track(window);
            this.evaluate();
        });
        this._workspaceRemovedId = this._workspace.connect('window-removed', (_workspace, window) => {
            this._untrack(window);
            this.evaluate();
        });
        for (const window of this._workspace.list_windows())
            this._track(window);
        this.evaluate();
    }

    _track(window) {
        if (this._windows.has(window) || window.skip_taskbar || window.title?.includes(RENDERER_ID))
            return;
        this._windows.set(window, [
            window.connect('notify::fullscreen', () => this.evaluate()),
            window.connect('notify::minimized', () => this.evaluate()),
        ]);
    }

    _untrack(window) {
        for (const signal of this._windows.get(window) ?? [])
            window.disconnect(signal);
        this._windows.delete(window);
    }

    _unbindWorkspace() {
        for (const window of [...this._windows.keys()])
            this._untrack(window);
        if (this._workspaceAddedId)
            this._workspace.disconnect(this._workspaceAddedId);
        if (this._workspaceRemovedId)
            this._workspace.disconnect(this._workspaceRemovedId);
        this._workspaceAddedId = 0;
        this._workspaceRemovedId = 0;
        this._workspace = null;
    }

    evaluate() {
        const fullscreen = [...this._windows.keys()].some(window =>
            window.fullscreen && !window.minimized
        );
        const gameMode = property(this._gameMode, 'ClientCount', 0) > 0;
        const powerSaver = property(this._powerProfile, 'power-saver-enabled', false);
        const onBattery = property(this._upower, 'OnBattery', false);
        const percentage = property(this._battery, 'Percentage', 100);
        const lowBattery = onBattery && percentage <= this._settings.get_uint('low-battery-threshold');
        const paused =
            (fullscreen && this._settings.get_boolean('pause-fullscreen')) ||
            (gameMode && this._settings.get_boolean('pause-game-mode')) ||
            (powerSaver && this._settings.get_boolean('pause-power-saver')) ||
            (lowBattery && this._settings.get_boolean('pause-low-battery'));
        if (paused !== this._paused) {
            this._paused = paused;
            this._changed(paused);
        }
        return paused;
    }

    disable() {
        this._unbindWorkspace();
        for (const [object, signal] of this._signals)
            object.disconnect(signal);
        for (const [proxy, signal] of this._proxies)
            proxy.disconnect(signal);
        if (this._settingsId)
            this._settings.disconnect(this._settingsId);
        this._signals = [];
        this._proxies = [];
        this._settingsId = 0;
    }
}

function property(proxy, name, fallback) {
    return proxy?.get_cached_property(name)?.deep_unpack() ?? fallback;
}