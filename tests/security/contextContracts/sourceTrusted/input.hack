<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\Source('UriRequestHeader'),
  Hakana\SecurityAnalysis\NotSourceWhen('key', 'trusted', 'also-trusted')>>
function source(string $key, string $fallback = ''): string { return $fallback; }
echo source('trusted');
