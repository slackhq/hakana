import { workspace, WorkspaceConfiguration } from 'vscode';
import { integer } from 'vscode-languageserver-protocol';
import { showOpenSettingsPrompt } from './utils';
import { LogLevel } from './LoggingService';

interface Config {
    path?: string;
    maxRestartCount: integer;
    connectToServerWithTcp: boolean;
    logLevel: LogLevel;
    hideStatusMessageWhenRunning: boolean;
    useDocker: boolean,
    dockerContainer?: string,
}

export class ConfigurationService {
    private config: Config = {
        maxRestartCount: 5,
        connectToServerWithTcp: false,
        hideStatusMessageWhenRunning: false,
        logLevel: 'TRACE',
        useDocker: false,
    };

    public constructor() { }

    public async init() {
        const workspaceConfiguration: WorkspaceConfiguration =
            workspace.getConfiguration('hakana');

        this.config.path = workspaceConfiguration.get('path', 'hakana-language-server');

        this.config.maxRestartCount = workspaceConfiguration.get(
            'maxRestartCount',
            5
        );

        this.config.connectToServerWithTcp = workspaceConfiguration.get(
            'connectToServerWithTcp',
            false
        );

        this.config.logLevel = workspaceConfiguration.get('logLevel', 'INFO');

        this.config.hideStatusMessageWhenRunning = workspaceConfiguration.get(
            'hideStatusMessageWhenRunning',
            false
        );

        this.config.useDocker = workspaceConfiguration.get('useDocker', false);
        this.config.dockerContainer = workspaceConfiguration.get('docker.containerName');
    }

    public async validate(): Promise<boolean> {
        // Check if the hakanaServerScriptPath setting was provided.
        if (!this.config.path) {
            await showOpenSettingsPrompt(
                'The setting hakana.path must be provided (e.g. hakana-language-server)'
            );
            return false;
        }
        return true;
    }

    public get<S extends keyof Config>(key: S): Config[S] {
        if (!(key in this.config)) {
            throw new Error(`Key ${key} not found in configuration`);
        }
        return this.config[key];
    }

    public getAll(): Config {
        return this.config;
    }
}
