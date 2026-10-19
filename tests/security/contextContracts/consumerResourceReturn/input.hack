<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function resource_url(<<Hakana\SecurityAnalysis\Sink('HtmlActiveResourceUri')>> string $url): string { return $url; }
function script(<<Hakana\SecurityAnalysis\Sink('JavaScript')>> string $code): void {}
script(resource_url(input()));
