<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\Source('NonUriRequestHeader')>>
function host(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
function run(): void {
    $url = 'https://' . host() . '/items/';
    fetch($url . '?page=' . input());
}
