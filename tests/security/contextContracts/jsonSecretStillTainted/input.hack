<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return ''; }
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
echo encode(secret());
