<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
function prefix(bool $again): string {
    if ($again) { return prefix(false); }
    return 'https://';
}
function run(bool $again): void {
    fetch(prefix($again) . input());
}
