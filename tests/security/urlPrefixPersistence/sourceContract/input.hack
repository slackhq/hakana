<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\Source('NonUriRequestHeader')>>
function prefix(): string { return 'https://example.com/'; }
function fetch(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $url): void {}
function run(): void { fetch(prefix() . input()); }
