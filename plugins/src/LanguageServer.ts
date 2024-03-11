import {
    LanguageClient,
    ErrorHandler,
    RevealOutputChannelOn,
} from 'vscode-languageclient/node';
import { StatusBar, LanguageServerStatus } from './StatusBar';
import { spawn, ChildProcess } from 'child_process';
import { workspace, Uri, Disposable } from 'vscode';
import { format, URL } from 'url';
import { ConfigurationService } from './ConfigurationService';
import LanguageServerErrorHandler from './LanguageServerErrorHandler';
import { LoggingService } from './LoggingService';
import { Writable } from 'stream';
import { showOpenSettingsPrompt } from './utils';

export class LanguageServer {
    private languageClient: LanguageClient;
    private workspacePath: string;
    private statusBar: StatusBar;
    private configurationService: ConfigurationService;
    private hakanaConfigPath: string;
    private debug: boolean;
    private loggingService: LoggingService;
    private ready = false;
    private initalizing = false;
    private disposable: Disposable;
    private serverProcess: ChildProcess | null = null;
    private restartCount: number = 0;

    constructor(
        workspacePath: string,
        hakanaConfigPath: string,
        statusBar: StatusBar,
        configurationService: ConfigurationService,
        loggingService: LoggingService
    ) {
        this.workspacePath = workspacePath;
        this.statusBar = statusBar;
        this.configurationService = configurationService;
        this.hakanaConfigPath = hakanaConfigPath;
        this.loggingService = loggingService;

        this.initClient([]);
    }

    private initClient(args: string[]) {
        this.languageClient = new LanguageClient(
            'hakana',
            'Hakana Language Server',
            this.spawnServer.bind(this),
            {
                outputChannel: this.loggingService,
                traceOutputChannel: this.loggingService,
                revealOutputChannelOn: RevealOutputChannelOn.Never,
                uriConverters: {
                    // VS Code by default %-encodes even the colon after the drive letter
                    // NodeJS handles it much better
                    code2Protocol: (uri: Uri): string => format(new URL(uri.toString(true))),
                    protocol2Code: (str: string): Uri => Uri.parse(str),
                },
                synchronize: {
                    // Synchronize the setting section 'hakana' to the server
                    configurationSection: 'hakana',
                    fileEvents: [
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

    /**
     * This will NOT restart the server.
     * @param workspacePath
     */
    public setWorkspacePath(workspacePath: string): void {
        this.workspacePath = workspacePath;
    }

    /**
     * This will NOT restart the server.
     * @param hakanaConfigPath
     */
    public setHakanaConfigPath(hakanaConfigPath: string): void {
        this.hakanaConfigPath = hakanaConfigPath;
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
        this.initalizing = false;
        this.loggingService.logInfo('Stopping language server');
        await this.languageClient.stop();
    }

    public async start(fromManualRestart: boolean) {
        this.initalizing = true;
        this.statusBar.update(LanguageServerStatus.Initializing, 'starting');
        this.loggingService.logInfo('Starting language server');

        if (fromManualRestart) {
            // re-initialises the client object, ensuring we pass in the correct args
            this.initClient(['--from-restart']);
        } else if (this.restartCount > 0) {
            this.initClient([]);
        }

        await this.languageClient.start();
        this.statusBar.update(LanguageServerStatus.Initialized, 'Ready');
        this.initalizing = false;
        this.ready = true;
        this.loggingService.logInfo(JSON.stringify(this.languageClient.initializeResult?.capabilities.textDocumentSync ?? ''));
        this.loggingService.logInfo('The Language Server is ready');
        this.restartCount++;
    }

    public async restart() {
        this.loggingService.logInfo('Restarting language server');
        await this.stop();
        await this.start(false);
    }

    public getLanguageClient(): LanguageClient {
        return this.languageClient;
    }

    /**
     * Spawn the Language Server as a child process
     * @param args Extra arguments to pass to the server
     * @return Promise<ChildProcess> A promise that resolves to the spawned process
     */
    private async spawnServer(args: string[] = []): Promise<ChildProcess> {
        this.loggingService.logInfo('Config file should be ' + this.hakanaConfigPath);

        const { file, args: fileArgs } = await this.getHakanaPath(args);

        this.loggingService.logInfo('Spawning ' + file + ' with cwd ' + this.workspacePath + ' and args ' + args.toString());

        const childProcess = spawn(file, fileArgs, {
            cwd: this.workspacePath,
        });
        this.serverProcess = childProcess;
        childProcess.stderr.on('data', (chunk: Buffer) => {
            this.loggingService.logError(chunk + '');
        });
        if (this.loggingService.getOutputLevel() === 'TRACE') {
            const orig = childProcess.stdin;

            childProcess.stdin = new Writable();
            // @ts-ignore
            childProcess.stdin.write = (chunk, encoding, callback) => {
                this.loggingService.logDebug(
                    chunk.toString ? `SERVER <== ${chunk.toString()}\n` : chunk
                );
                return orig.write(chunk, encoding, callback);
            };

            childProcess.stdout.on('data', (chunk: Buffer) => {
                this.loggingService.logDebug(`SERVER ==> ${chunk}\n`);
            });
        }

        childProcess.on('exit', (code, signal) => {
            this.loggingService.logInfo('Exited with ' + code + ' and signal ' + signal?.toString());
            if (code != 0) {
                this.statusBar.update(
                    LanguageServerStatus.Exited,
                    'Exited (Should Restart)'
                );
            }
        });
        return childProcess;
    }

    /**
     * Get the Hakana executable Location and Arguments to pass to Hakana
     *
     * @param args The arguments to pass to Hakana
     */
    private async getHakanaPath(
        args: string[]
    ): Promise<{ file: string; args: string[] }> {
        let executablePath =
            this.configurationService.get('path') || 'hakana-language-server';

        if (!executablePath.length) {
            const msg =
                'Unable to find any Hakana executable — please set one in hakana.path';
            await showOpenSettingsPrompt(`Hakana can not start: ${msg}`);
            throw new Error(msg);
        }

        const useDocker = this.configurationService.get('useDocker');

        if (useDocker) {
            args = ["exec", "-i", this.configurationService.get('dockerContainer') || '', executablePath, ...args];
            executablePath = 'docker';
        }

        return { file: executablePath, args };
    }
}
