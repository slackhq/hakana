import {
    LanguageClient,
    ErrorHandler,
    RevealOutputChannelOn,
    ServerOptions,
} from 'vscode-languageclient/node';
import { StatusBar, LanguageServerStatus } from './StatusBar';
import { ChildProcess } from 'child_process';
import { workspace, Uri, Disposable } from 'vscode';
import { format, URL } from 'url';
import { ConfigurationService } from './ConfigurationService';
import LanguageServerErrorHandler from './LanguageServerErrorHandler';
import { LoggingService } from './LoggingService';

export class LanguageServer {
    private languageClient: LanguageClient;
    private statusBar: StatusBar;
    private configurationService: ConfigurationService;
    private debug: boolean;
    private loggingService: LoggingService;
    private ready = false;
    private initalizing = false;
    private disposable: Disposable;
    private serverProcess: ChildProcess | null = null;

    constructor(
        workspacePath: string,
        hakanaConfigPath: string,
        statusBar: StatusBar,
        configurationService: ConfigurationService,
        loggingService: LoggingService
    ) {
        this.statusBar = statusBar;
        this.configurationService = configurationService;
        this.loggingService = loggingService;

        const { file, args: fileArgs } = this.getHakanaPath([]);

        const serverOptions: ServerOptions = {
            command: file,
            args: fileArgs,
        };

        this.languageClient = new LanguageClient(
            'hakana',
            'Hakana Language Server',
            serverOptions,
            {
                outputChannel: this.loggingService,
                traceOutputChannel: this.loggingService,
                revealOutputChannelOn: RevealOutputChannelOn.Never,
                uriConverters: {
                    // VS Code by default %-encodes even the colon after the drive letter
                    // NodeJS handles it much better
                    code2Protocol: (uri: Uri): string =>
                        format(new URL(uri.toString(true))),
                    protocol2Code: (str: string): Uri => Uri.parse(str),
                },
                synchronize: {
                    // Synchronize the setting section 'hakana' to the server
                    configurationSection: 'hakana',
                    fileEvents: [
                        // this is for when files get changed outside of vscode
                        workspace.createFileSystemWatcher('**/*.(php|hack|hhi)'),
                        workspace.createFileSystemWatcher('**/hakana.json'),
                    ],
                },
                progressOnInitialization: true,
                errorHandler: this.createDefaultErrorHandler(
                    this.configurationService.get('maxRestartCount') - 1
                ),
            },
            this.debug
        );

        this.languageClient.onTelemetry(this.onTelemetry.bind(this));
    }

    public createDefaultErrorHandler(maxRestartCount?: number): ErrorHandler {
        if (maxRestartCount !== undefined && maxRestartCount < 0) {
            throw new Error(`Invalid maxRestartCount: ${maxRestartCount}`);
        }
        return new LanguageServerErrorHandler(
            'Hakana Language Server',
            maxRestartCount ?? 4
        );
    }

    private onTelemetry(params: any) {
        if (
            typeof params === 'object' &&
            'message' in params &&
            typeof params.message === 'string'
        ) {
            // each time we get a new telemetry, we are going to check the config, and update as needed
            const hideStatusMessageWhenRunning = this.configurationService.get(
                'hideStatusMessageWhenRunning'
            );

            this.loggingService.logInfo(params.message);

            let status: string = params.message;

            if (params.message.indexOf(':') >= 0) {
                status = params.message.split(':')[0];
            }

            switch (status) {
                case 'initializing':
                    this.statusBar.update(
                        LanguageServerStatus.Initializing,
                        params.message
                    );
                    break;
                case 'initialized':
                    this.statusBar.update(
                        LanguageServerStatus.Initialized,
                        params.message
                    );
                    break;
                case 'running':
                    this.statusBar.update(
                        LanguageServerStatus.Running,
                        params.message
                    );
                    break;
                case 'analyzing':
                    this.statusBar.update(
                        LanguageServerStatus.Analyzing,
                        params.message
                    );
                    break;
                case 'closing':
                    this.statusBar.update(
                        LanguageServerStatus.Closing,
                        params.message
                    );
                    break;
                case 'closed':
                    this.statusBar.update(
                        LanguageServerStatus.Closed,
                        params.message
                    );
                    break;
            }

            if (hideStatusMessageWhenRunning && status === 'running') {
                this.statusBar.hide();
            } else {
                this.statusBar.show();
            }
        }
    }

    public getServerProcess(): ChildProcess | null {
        return this.serverProcess;
    }

    public isReady(): boolean {
        return this.ready;
    }

    public isInitalizing(): boolean {
        return this.initalizing;
    }

    public getDisposable(): Disposable {
        return this.disposable;
    }

    public getClient(): LanguageClient {
        return this.languageClient;
    }

    public async stop() {
        if (this.initalizing) {
            this.loggingService.logWarning(
                'Server is in the process of intializing'
            );
            return;
        }
        this.loggingService.logInfo('Stopping language server');
        await this.languageClient.stop();
    }

    public async start() {
        this.initalizing = true;
        this.statusBar.update(LanguageServerStatus.Initializing, 'starting');
        this.loggingService.logInfo('Starting language server');
        await this.languageClient.start();
        this.statusBar.update(LanguageServerStatus.Initialized, 'Ready');
        this.initalizing = false;
        this.ready = true;
        this.loggingService.logInfo(JSON.stringify(this.languageClient.initializeResult?.capabilities.textDocumentSync ?? ''));
        this.loggingService.logInfo('The Language Server is ready');
    }

    public async restart() {
        this.loggingService.logInfo('Restarting language server');
        await this.stop();
        await this.start();
    }

    public getLanguageClient(): LanguageClient {
        return this.languageClient;
    }

    /**
     * Get the PHP Executable Location and Arguments to pass to PHP
     *
     * @param args The arguments to pass to PHP
     */
    private getHakanaPath(
        args: string[]
    ): { file: string; args: string[] } {
        let executablePath =
            this.configurationService.get('path') || 'hakana-language-server';

        const useDocker = this.configurationService.get('useDocker');

        if (useDocker) {
            args = ["exec", "-i", this.configurationService.get('dockerContainer') || '', executablePath, ...args];
            executablePath = 'docker';
        }

        return { file: executablePath, args };
    }
}
