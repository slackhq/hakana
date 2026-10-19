<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function resource_url(<<Hakana\SecurityAnalysis\Sink('HtmlActiveResourceUri')>> string $url): string { return $url; }
function script(<<Hakana\SecurityAnalysis\Sink('JavaScript')>> string $code): void {}
resource_url(input());
script(resource_url('safe'));
