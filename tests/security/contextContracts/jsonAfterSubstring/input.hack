<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function script(<<Hakana\SecurityAnalysis\Sink('JavaScript')>> string $code): void {}
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
script(substr(encode(input()), 1, -1));
