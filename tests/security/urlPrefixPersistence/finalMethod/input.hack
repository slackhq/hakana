<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
final class Builder {
    public function url(string $item): string { return 'https://example.com/' . $item; }
}
function run(Builder $builder): void {
    fetch($builder->url(input()) . '?page=' . input());
}
