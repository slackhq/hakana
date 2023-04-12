import { workspace, WorkspaceConfiguration } from 'vscode';
import { integer } from 'vscode-languageserver-protocol';
import { showOpenSettingsPrompt } from './utils';
import { LogLevel } from './LoggingService';

interface Config {
    hakanaPath?: string;
    maxRestartCount: integer;
    connectToServerWithTcp: boolean;
    logLevel: LogLevel;
    hideStatusMessageWhenRunning: boolean;
}

export class ConfigurationService {
    private config: Config = {
        maxRestartCount: 5,
        connectToServerWithTcp: false,
        hideStatusMessageWhenRunning: false,
        logLevel: 'TRACE',
    };

    public constructor() { }

    public async init() {
        const workspaceConfiguration: WorkspaceConfiguration =
            workspace.getConfiguration('hakana');

        this.config.hakanaPath = workspaceConfiguration.get('hakanaPath', 'hakana-language-server');

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
    }

    public async validate(): Promise<boolean> {
        // Check if the hakanaServerScriptPath setting was provided.
        if (!this.config.hakanaPath) {
            await showOpenSettingsPrompt(
                'The setting hakana.hakanaPath must be provided (e.g. vendor/bin/hakana-language-server)'
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
