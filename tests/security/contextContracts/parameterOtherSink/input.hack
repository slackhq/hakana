<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function url(<<Hakana\SecurityAnalysis\Sink('CurlUri')>> string $s): void {}
function root_url(string $base, <<Hakana\SecurityAnalysis\Sanitize('CurlUri')>> string $path): string { return $base . $path; }
echo root_url('https://example.org', input());
