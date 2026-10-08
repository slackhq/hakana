<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
function url(string $item): string { return 'https://example.com/items/' . $item; }
function run(): void {
    $url = substr(url(input()), 4);
    fetch($url . input());
}
