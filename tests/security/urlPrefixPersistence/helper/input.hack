<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
function url(string $item): string { return 'https://example.com/items/' . $item; }
function wrapper(string $item): string { return url($item); }
function run(): void {
    $url = wrapper(input());
    $url .= '?page=' . input();
    fetch($url);
}
