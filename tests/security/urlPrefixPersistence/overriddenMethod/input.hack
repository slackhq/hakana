<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
class Builder {
    public function url(): string { return 'https://example.com/'; }
}
final class Other extends Builder {
    <<__Override>>
    public function url(): string { return 'https://'; }
}
function run(Builder $builder): void {
    fetch($builder->url() . input());
}
