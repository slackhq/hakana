import * as vscode from 'vscode';
import { StatusBar } from './StatusBar';
import { LoggingService } from './LoggingService';
import { ConfigurationService } from './ConfigurationService';
import { LanguageServer } from './LanguageServer';
import { registerCommands } from './commands';
import { showWarningMessage } from './utils';

/**
 * Activate the extension.
 *
 * NOTE: This is only ever run once so it's safe to listen to events here
 */
export async function activate(
    context: vscode.ExtensionContext
): Promise<void> {
    // @ts-ignore
    const loggingService = new LoggingService();
    // @ts-ignore
    const configurationService = new ConfigurationService();
    await configurationService.init();

    // Set Logging Level
    loggingService.setOutputLevel(configurationService.get('logLevel'));

    // @ts-ignore
    const statusBar = new StatusBar();

    const workspaceFolders = vscode.workspace.workspaceFolders;

    if (!workspaceFolders) {
        loggingService.logError(
            'Hakana must be run in a workspace. Select a workspace and reload the window'
        );
        return;
    }

    const getCurrentWorkspace = (
        workspaceFolders: readonly vscode.WorkspaceFolder[]
    ) => {
        const activeWorkspace = vscode.window.activeTextEditor
            ? vscode.workspace.getWorkspaceFolder(
                vscode.window.activeTextEditor.document.uri
            )
            : workspaceFolders[0];

        const workspacePath = activeWorkspace
            ? activeWorkspace.uri.fsPath
            : workspaceFolders[0].uri.fsPath;

        return { workspacePath };
    };

    let workspacePath = await getCurrentWorkspace(workspaceFolders).workspacePath;

    let configPath = workspacePath + '/hakana.json';

    let configWatcher = vscode.workspace.createFileSystemWatcher(configPath);

    const languageServer = new LanguageServer(
        workspacePath,
        configPath,
        statusBar,
        configurationService,
        loggingService
    );

    // restart the language server when changing workspaces
    const onWorkspacePathChange = async () => {
        //kill the previous watcher
        configWatcher.dispose();
        configWatcher = vscode.workspace.createFileSystemWatcher(configPath);
        loggingService.logInfo(`Workspace changed: ${workspacePath}`);
        languageServer.setWorkspacePath(workspacePath);
        languageServer.setHakanaConfigPath(configPath);
        languageServer.restart();
    };

    const onConfigChange = () => {
        loggingService.logInfo(`Config file changed: ${configPath}`);
        languageServer.restart();
    };

    const onConfigDelete = () => {
        loggingService.logInfo(`Config file deleted: ${configPath}`);
        languageServer.stop();
    };

    // Restart the language server when the tracked config file changes
    configWatcher.onDidChange(onConfigChange);
    configWatcher.onDidCreate(onConfigChange);
    configWatcher.onDidDelete(onConfigDelete);

    context.subscriptions.push(
        ...registerCommands(
            languageServer,
            configurationService,
            loggingService
        )
    );

    // Start Lanuage Server
    await languageServer.start(false);

    vscode.workspace.onDidChangeConfiguration(async (change) => {
        if (
            !change.affectsConfiguration('hakana') ||
            change.affectsConfiguration('hakana.hideStatusMessageWhenRunning')
        ) {
            return;
        }
        loggingService.logDebug('Configuration changed');
        showWarningMessage(
            'You will need to reload this window for the new configuration to take effect'
        );

        await configurationService.init();
    });

    vscode.window.onDidChangeActiveTextEditor(async (e) => {
        if (!e) {
            return;
        }

        const newWorkspacePath = await getCurrentWorkspace(workspaceFolders).workspacePath;

        if (!newWorkspacePath || workspacePath === newWorkspacePath)
            return;

        workspacePath = newWorkspacePath;

        onWorkspacePathChange();
    });

    loggingService.logDebug('Finished Extension Activation');
}

export async function deactivate() {
    //Extensions should now implement a deactivate function in their extension main file and correctly return the stop promise from the deactivate call.
}
