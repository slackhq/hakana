import {
    Message,
    ErrorHandler,
    ErrorAction,
    ErrorHandlerResult,
    CloseAction,
    CloseHandlerResult,
} from 'vscode-languageclient/node';
import { showReportIssueErrorMessage } from './utils';

export default class LanguageServerErrorHandler implements ErrorHandler {
    private readonly restarts: number[];

    constructor(private name: string, private maxRestartCount: number) {
        this.restarts = [];
    }

    public error(
        error: Error,
        message: Message | undefined,
        count: number | undefined
    ): ErrorHandlerResult {
        if (count && count <= 3) {
            return {
                action: ErrorAction.Continue,
            };
        }
        return {
            action: ErrorAction.Shutdown,
        };
    }

    public closed(): CloseHandlerResult {
        this.restarts.push(Date.now());
        if (this.restarts.length <= this.maxRestartCount) {
            return { action: CloseAction.Restart };
        } else {
            const diff =
                this.restarts[this.restarts.length - 1] - this.restarts[0];
            if (diff <= 3 * 60 * 1000) {
                const message = `The ${this.name} server crashed ${
                    this.maxRestartCount + 1
                } times in the last 3 minutes. The server will not be restarted.`;
                showReportIssueErrorMessage(message);
                return { action: CloseAction.DoNotRestart, message: message };
            } else {
                this.restarts.shift();
                return { action: CloseAction.Restart };
            }
        }
    }
}
