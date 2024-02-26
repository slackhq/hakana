import * as vscode from 'vscode';
import { LanguageServer } from './LanguageServer';
import * as path from 'path';
import { EXTENSION_ROOT_DIR } from './constants';
import { formatFromTemplate } from './utils';
import { ConfigurationService } from './ConfigurationService';
import { LoggingService } from './LoggingService';
import { EOL } from 'os';
interface Command {
    id: string;
    execute(): void;
}

async function restartSever(
    client: LanguageServer,
    configurationService: ConfigurationService
) {
    await client.stop();
    client.start(true);
}

function analyzeWorkSpace(
    client: LanguageServer,
    configurationService: ConfigurationService
): Command {
    return {
        id: 'hakana.analyzeWorkSpace',
        async execute() {
            return await restartSever(client, configurationService);
        },
    };
}

function restartHakanaServer(
    client: LanguageServer,
    configurationService: ConfigurationService
): Command {
    return {
        id: 'hakana.restartHakanaServer',
        async execute() {
            return await restartSever(client, configurationService);
        },
    };
}

function reportIssue(
    client: LanguageServer,
    configurationService: ConfigurationService,
    loggingService: LoggingService
): Command {
    return {
        id: 'hakana.reportIssue',
        async execute() {
            const templatePath = path.join(
                EXTENSION_ROOT_DIR,
                'resources',
                'report_issue_template.md'
            );

            const userSettings = Object.entries(configurationService.getAll())
                .map(([key, value]) => `${key}: ${JSON.stringify(value)}`)
                .join(EOL);
            const hakanaLogs = loggingService.getContent().join(EOL);

            await vscode.commands.executeCommand(
                'workbench.action.openIssueReporter',
                {
                    extensionId: 'gethakana.hakana-vscode-plugin',
                    issueBody: await formatFromTemplate(
                        templatePath,
                        hakanaLogs, // 2
                        userSettings // 3
                    ),
                }
            );
        },
    };
}

function showOutput(loggingService: LoggingService): Command {
    return {
        id: 'hakana.showOutput',
        async execute() {
            loggingService.show();
        },
    };
}

export function registerCommands(
    client: LanguageServer,
    configurationService: ConfigurationService,
    loggingService: LoggingService
): vscode.Disposable[] {
    const commands: Command[] = [
        restartHakanaServer(client, configurationService),
        analyzeWorkSpace(client, configurationService),
        reportIssue(client, configurationService, loggingService),
        showOutput(loggingService),
    ];

    const disposables = commands.map((command) => {
        return vscode.commands.registerCommand(command.id, command.execute);
    });

    return disposables;
}
