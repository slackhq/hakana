<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
function run(bool $first, string $other): void {
    $url = $first ? '/items/' . input() : $other;
    fetch($url . input());
}
