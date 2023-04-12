import { window, commands } from 'vscode';
import * as fs from 'fs-extra';

export async function showOpenSettingsPrompt(
    errorMessage: string
): Promise<void> {
    const selected = await window.showErrorMessage(
        errorMessage,
        'Open settings'
    );
    if (selected === 'Open settings') {
        await commands.executeCommand('workbench.action.openGlobalSettings');
    }
}

export async function showReportIssueErrorMessage(
    errorMessage: string
): Promise<void> {
    const selected = await window.showErrorMessage(
        errorMessage,
        'Report Issue'
    );
    if (selected === 'Report Issue') {
        await commands.executeCommand('hakana.reportIssue');
    }
}

export async function showErrorMessage(
    errorMessage: string
): Promise<string | undefined> {
    return window.showErrorMessage(errorMessage);
}

export async function showWarningMessage(
    errorMessage: string
): Promise<string | undefined> {
    return window.showWarningMessage(errorMessage);
}

export async function formatFromTemplate(templatePath: string, ...args: any[]) {
    const template = await fs.readFile(templatePath, 'utf8');

    return template.replace(/{(\d+)}/g, (match, number) =>
        args[number] === undefined ? match : args[number]
    );
}
