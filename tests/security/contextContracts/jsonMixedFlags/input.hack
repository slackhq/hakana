<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function script(<<Hakana\SecurityAnalysis\Sink('JavaScript')>> string $code): void {}
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
$flags = rand() > 0 ? JSON_HEX_TAG : 0;
script(json_encode(input(), $flags) as string);
