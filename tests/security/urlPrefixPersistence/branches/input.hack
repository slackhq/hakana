<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
function run(bool $first): void {
    $url = $first ? '/items/' . input() : '/other/' . input();
    fetch($url . '?page=' . input());
}
