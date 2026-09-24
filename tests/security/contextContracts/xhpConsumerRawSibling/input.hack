use type Facebook\XHP\HTML\script;
<<Hakana\SecurityAnalysis\Source('UriRequestHeader'), Hakana\SecurityAnalysis\SpecializeCall>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
<<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
function render(): script {
    $encoded = encode(input());
    $raw = input();
    return <script>{'window.a = '.$encoded.'; window.b = '.$raw.';'}</script>;
}
render();
