use type Facebook\XHP\HTML\script;
<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
function unchecked(string $code): script {
    return <script>{$code}</script>;
}
<<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
function render(): script {
    return unchecked('window.a = '.encode(input()).';');
}
render();
