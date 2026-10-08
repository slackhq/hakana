<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
function reset(inout string $url): void { $url = 'https://'; }
function run(): void {
    $url = 'https://example.com/items/' . input();
    reset(inout $url);
    fetch($url . input());
}
