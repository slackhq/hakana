use type Facebook\XHP\HTML\script;
<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
function data(): string {
    $a = encode(input());
    $b = encode(input());
    $js = 'window.a = '.$a.';';
    $js .= 'window.b = '.$b.';';
    return '(function () {'.$js.'})();';
}
final class Page {
    <<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
    public static function render(): script {
        return <script>{data()}</script>;
    }
}
Page::render();
