import {
    LanguageClient,
    StreamInfo,
    ErrorHandler,
    RevealOutputChannelOn,
} from 'vscode-languageclient/node';
import { StatusBar, LanguageServerStatus } from './StatusBar';
import { spawn, ChildProcess } from 'child_process';
import { workspace, Uri, Disposable } from 'vscode';
import { format, URL } from 'url';
import { ConfigurationService } from './ConfigurationService';
import LanguageServerErrorHandler from './LanguageServerErrorHandler';
import { statSync, constants } from 'fs';
import { access } from 'fs/promises';
import { LoggingService } from './LoggingService';
import { Writable } from 'stream';
import { createServer } from 'net';
import { showOpenSettingsPrompt, showErrorMessage } from './utils';

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

        this.languageClient = new LanguageClient(
            'hakana',
            'Hakana Language Server',
            this.serverOptions.bind(this),
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
        // Check if hakana is installed and supports the language server protocol.
        const isValidHakanaVersion: boolean =
            await this.checkHakanaHasLanguageServer();
        if (!isValidHakanaVersion) {
            showOpenSettingsPrompt('Hakana is not installed');
            return;
        }

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

    private serverOptions(): Promise<ChildProcess | StreamInfo> {
        return new Promise<ChildProcess | StreamInfo>((resolve, reject) => {
            const connectToServerWithTcp = this.configurationService.get(
                'connectToServerWithTcp'
            );

            if (connectToServerWithTcp) {
                const server = createServer((socket) => {
                    // 'connection' listener
                    this.loggingService.logDebug('PHP process connected');
                    socket.on('end', () => {
                        this.loggingService.logDebug(
                            'PHP process disconnected'
                        );
                    });

                    if (this.loggingService.getOutputLevel() === 'TRACE') {
                        socket.on('data', (chunk: Buffer) => {
                            this.loggingService.logDebug(
                                `SERVER ==> ${chunk}\n`
                            );
                        });
                    }

                    const writeable = new Writable();

                    // @ts-ignore
                    writeable.write = (chunk, encoding, callback) => {
                        if (this.loggingService.getOutputLevel() === 'TRACE') {
                            this.loggingService.logDebug(
                                chunk.toString
                                    ? `SERVER <== ${chunk.toString()}\n`
                                    : chunk
                            );
                        }
                        return socket.write(chunk, encoding, callback);
                    };

                    server.close();
                    resolve({ reader: socket, writer: writeable });
                });
                server.listen(0, '127.0.0.1', () => {
                    // Start the language server
                    // make the language server connect to the client listening on <addr> (e.g. 127.0.0.1:<port>)
                    this.spawnServer([
                        // @ts-ignore
                        '--tcp=127.0.0.1:' + server.address().port,
                    ]);
                });
            } else {
                // Use STDIO on Linux / Mac if the user set
                // the override `"hakana.connectToServerWithTcp": false` in their config.
                resolve(this.spawnServer());
            }
        });
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
            this.statusBar.update(
                LanguageServerStatus.Exited,
                'Exited (Should Restart)'
            );
        });
        return childProcess;
    }

    /**
     * Get the PHP Executable Location and Arguments to pass to PHP
     *
     * @param args The arguments to pass to PHP
     */
    private async getHakanaPath(
        args: string[]
    ): Promise<{ file: string; args: string[] }> {
        let executablePath =
            this.configurationService.get('path');

        if (!executablePath || !executablePath.length) {
            const msg =
                'Unable to find any Hakana executable please set one in hakana.path';
            await showOpenSettingsPrompt(`Hakana can not start: ${msg}`);
            throw new Error(msg);
        }

        const useDocker = this.configurationService.get('useDocker');

        if (useDocker) {
            args = ["exec", "-i", this.configurationService.get('dockerContainer') || '', executablePath, ...args];
            executablePath = 'docker';
        }

        try {
            await access(executablePath, constants.X_OK);
        } catch {
            const msg = `${executablePath} is not executable`;
            await showErrorMessage(`Hakana can not start: ${msg}`);
            throw new Error(msg);
        }

        return { file: executablePath, args };
    }

    /**
     * Returns true if hakana.path supports the language server protocol.
     * @return Promise<boolean> A promise that resolves to true if the language server protocol is supported
     */
    private async checkHakanaHasLanguageServer(): Promise<boolean> {
        const { file: hakanaPath } = await this.getHakanaPath([]);

        const exists: boolean = this.isFile(hakanaPath);

        if (!exists) {
            this.loggingService.logError(
                `The setting hakana.path refers to a path that does not exist. path: ${hakanaPath}`
            );
            return false;
        }

        return true;
    }

    /**
     * Returns true if the file exists.
     */
    private isFile(filePath: string): boolean {
        try {
            const stat = statSync(filePath);
            return stat.isFile();
        } catch (e) {
            return false;
        }
    }
}
