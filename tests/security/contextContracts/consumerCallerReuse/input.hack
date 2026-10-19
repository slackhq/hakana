<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
<<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
function checked(<<Hakana\SecurityAnalysis\Sink('JavaScript')>> string $code): string { return $code; }
function unchecked(<<Hakana\SecurityAnalysis\Sink('JavaScript')>> string $code): void {}
$code = 'window.a = '.encode(input()).';';
unchecked(checked($code));
