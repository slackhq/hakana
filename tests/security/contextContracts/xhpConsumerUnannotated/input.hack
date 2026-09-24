use type Facebook\XHP\HTML\script;
<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
function render(): script {
    return <script>{'window.a = '.encode(input()).';'}</script>;
}
render();
