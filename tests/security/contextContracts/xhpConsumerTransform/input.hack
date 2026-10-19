use type Facebook\XHP\HTML\script;
<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
<<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
function render(): script {
    $value = substr(encode(input()), 1, -1);
    return <script>{'window.a = '.$value.';'}</script>;
}
render();
