<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
function script(<<Hakana\SecurityAnalysis\Sink('JavaScript')>> string $code): void {}
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
script('var values = {a: ' . encode(input()) . ', b: [' . encode('static') . ']};');
