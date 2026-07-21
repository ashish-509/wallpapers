import Gio from 'gi://Gio';
import Meta from 'gi://Meta';

export class RendererClient {
    constructor(onExit) {
        this._onExit = onExit;
        this._waylandClient = null;
        this.process = null;
    }

    start(command, file) {
        const launcher = new Gio.SubprocessLauncher({
            flags: Gio.SubprocessFlags.STDOUT_SILENCE |
                Gio.SubprocessFlags.STDERR_SILENCE,
        });
        const argv = [...command, '--file', file];
        this._waylandClient = Meta.WaylandClient.new_subprocess(
            global.context,
            launcher,
            argv
        );
        this.process = this._waylandClient.get_subprocess();
        launcher.close?.();
        this.process.wait_async(null, (process, result) => {
            try {
                process.wait_finish(result);
            } catch (error) {
                console.error(`Motion renderer wait failed: ${error.message}`);
            }
            if (process === this.process)
                this._onExit();
        });
    }

    owns(window) {
        try {
            return this._waylandClient?.owns_window(window) ?? false;
        } catch {
            return false;
        }
    }

    stop() {
        this.process?.send_signal(15);
        this.process = null;
        this._waylandClient = null;
    }
}