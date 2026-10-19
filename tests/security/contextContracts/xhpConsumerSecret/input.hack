use type Facebook\XHP\HTML\script;
<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return ''; }
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
<<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
function render(): script {
    return <script>{'window.a = '.encode(secret()).';'}</script>;
}
render();
