<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
function script(<<Hakana\SecurityAnalysis\Sink('JavaScript')>> string $code): void {}
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
$encoded = rand() > 0 ? encode(input()) : input();
script('var value = ' . $encoded . ';');
