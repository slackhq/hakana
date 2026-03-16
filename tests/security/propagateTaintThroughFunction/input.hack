<<\Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function getInput(): string {
    return "";
}

function decode_id(<<\Hakana\SecurityAnalysis\PropagateTaint>> string $id): string {
    return "hardcoded";
}

function output(
    <<\Hakana\SecurityAnalysis\Sink('HtmlTag')>> string $data,
): void {}

function main(): void {
    $id = getInput();
    $decoded = decode_id($id);
    output($decoded);
}
