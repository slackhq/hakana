<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
function url(bool $first, string $other): string {
    if ($first) { return '/items/'; }
    return $other;
}
function run(bool $first, string $other): void {
    fetch(url($first, $other) . input());
}
