<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
function run(): void {
    $url = 'https://example.com/items/' . input();
    $copy = $url;
    $copy .= '?page=' . input();
    $copy = $copy . '&sort=' . input();
    fetch($copy);
}
