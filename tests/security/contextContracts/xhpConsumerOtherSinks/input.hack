use type Facebook\XHP\HTML\{script, a};
use type Facebook\XHP\Core\frag;
<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\HtmlSafeJson>>
function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
<<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
function render(): frag {
    $value = encode(input());
    return <frag>
        <script>{'window.a = '.$value.';'}</script>
        <script src={$value} />
        <a href={$value}>Link</a>
    </frag>;
}
render();
