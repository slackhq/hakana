import { StatusBarAlignment, StatusBarItem, ThemeColor, window } from 'vscode';

export enum LanguageServerStatus {
    Initializing = 'sync~spin',
    Initialized = 'zap',
    Running = 'check',
    Analyzing = 'sync~spin',
    Closing = 'issues',
    Closed = 'error',
    Exited = 'error',
}

export class StatusBar {
    private statusBarItem: StatusBarItem;
    constructor() {
        // Setup the statusBarItem
        this.statusBarItem = window.createStatusBarItem(
            'hakana.status',
            StatusBarAlignment.Left, // this has a low priority so it will end up being more towards the right.
            1
        );
        this.statusBarItem.name = 'Hakana';
        this.statusBarItem.tooltip = 'Hakana Language Server';
        this.statusBarItem.hide();
    }

    /**
     * Update the statusBarItem message and show the statusBarItem
     *
     * @param icon The the icon to use
     */
    public update(result: LanguageServerStatus, text: string): void {
        this.statusBarItem.text = `$(${result.toString()}) Hakana: ${text}`;
        if (result === LanguageServerStatus.Exited) {
            this.statusBarItem.backgroundColor = new ThemeColor(
                'statusBarItem.errorBackground'
            );
        } else {
            this.statusBarItem.backgroundColor = undefined;
        }
        this.show();
    }

    public show() {
        this.statusBarItem.show();
    }

    public hide() {
        this.statusBarItem.hide();
    }
}
