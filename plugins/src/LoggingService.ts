import { window, OutputChannel } from 'vscode';

export type LogLevel = 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR' | 'NONE';
export class LoggingService implements OutputChannel {
    private outputChannel = window.createOutputChannel('Hakana Language Server');

    private logLevel: LogLevel = 'DEBUG';

    private content: string[] = [];

    private contentLimit = 1000;

    readonly name: string = 'Hakana Language Server';

    /**
     * Replaces all output from the channel with the given value.
     *
     * @param value A string, falsy values will not be printed.
     */
    replace(value: string): void {
        this.content = [value];
        this.outputChannel.replace(value);
    }

    /**
     * Append the given value to the channel.
     *
     * @param value A string, falsy values will not be printed.
     */
    append(value: string): void {
        this.content.push(value);
        this.content = this.content.slice(-this.contentLimit);
        this.outputChannel.append(value);
    }

    /**
     * Append the given value and a line feed character
     * to the channel.
     *
     * @param value A string, falsy values will be printed.
     */
    appendLine(value: string): void {
        this.content.push(value);
        this.content = this.content.slice(-this.contentLimit);
        this.outputChannel.appendLine(value);
    }

    /**
     * Removes all output from the channel.
     */
    clear(): void {
        this.content = [];
        this.outputChannel.clear();
    }

    /**
     * Reveal this channel in the UI.
     *
     * @param preserveFocus When `true` the channel will not take focus.
     */
    show(): void {
        this.outputChannel.show(...arguments);
    }

    /**
     * Hide this channel from the UI.
     */
    hide(): void {
        this.outputChannel.hide();
    }

    /**
     * Dispose and free associated resources.
     */
    dispose(): void {
        this.outputChannel.dispose();
    }

    public setOutputLevel(logLevel: LogLevel) {
        this.logLevel = logLevel;
    }

    public getOutputLevel(): LogLevel {
        return this.logLevel;
    }

    public logTrace(message: string, data?: unknown): void {
        if (
            this.logLevel === 'NONE' ||
            this.logLevel === 'INFO' ||
            this.logLevel === 'WARN' ||
            this.logLevel === 'ERROR' ||
            this.logLevel === 'DEBUG'
        ) {
            return;
        }
        this.logMessage(message, 'TRACE');
        if (data) {
            this.logObject(data);
        }
    }

    /**
     * Append messages to the output channel and format it with a title
     *
     * @param message The message to append to the output channel
     */
    public logDebug(message: string, data?: unknown): void {
        if (
            this.logLevel === 'NONE' ||
            this.logLevel === 'INFO' ||
            this.logLevel === 'WARN' ||
            this.logLevel === 'ERROR'
        ) {
            return;
        }
        this.logMessage(message, 'DEBUG');
        if (data) {
            this.logObject(data);
        }
    }

    /**
     * Append messages to the output channel and format it with a title
     *
     * @param message The message to append to the output channel
     */
    public logInfo(message: string, data?: unknown): void {
        if (
            this.logLevel === 'NONE' ||
            this.logLevel === 'WARN' ||
            this.logLevel === 'ERROR'
        ) {
            return;
        }
        this.logMessage(message, 'INFO');
        if (data) {
            this.logObject(data);
        }
    }

    /**
     * Append messages to the output channel and format it with a title
     *
     * @param message The message to append to the output channel
     */
    public logWarning(message: string, data?: unknown): void {
        if (this.logLevel === 'NONE' || this.logLevel === 'ERROR') {
            return;
        }
        this.logMessage(message, 'WARN');
        if (data) {
            this.logObject(data);
        }
    }

    public logError(message: string, error?: Error | string) {
        if (this.logLevel === 'NONE') {
            return;
        }
        this.logMessage(message, 'ERROR');
        if (typeof error === 'string') {
            // Errors as a string usually only happen with
            // plugins that don't return the expected error.
            this.appendLine(error);
        } else if (error?.message || error?.stack) {
            if (error?.message) {
                this.logMessage(error.message, 'ERROR');
            }
            if (error?.stack) {
                this.appendLine(error.stack);
            }
        } else if (error) {
            this.logObject(error);
        }
    }

    public getContent(): string[] {
        return this.content;
    }

    private logObject(data: unknown): void {
        this.appendLine(JSON.stringify(data, null, 2));
    }

    /**
     * Append messages to the output channel and format it with a title
     *
     * @param message The message to append to the output channel
     */
    private logMessage(message: string, logLevel: LogLevel): void {
        const title = new Date().toLocaleTimeString();
        this.appendLine(`[${logLevel}  - ${title}] ${message}`);
    }
}
