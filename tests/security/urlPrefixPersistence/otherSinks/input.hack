<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return ''; }
function resource(<<Hakana\SecurityAnalysis\Sink('HtmlActiveResourceUri')>> string $url): void {}
function url(string $item): string { return 'https://example.com/items/' . $item; }
function prefix(): string { return 'https://example.com/items/'; }
function run(): void {
    resource(url(input()));
    resource(prefix() . '?page=' . input());
    echo url(secret());
    echo prefix() . '?page=' . secret();
}
