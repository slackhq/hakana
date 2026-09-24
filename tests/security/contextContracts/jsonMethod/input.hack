<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
function script(<<Hakana\SecurityAnalysis\Sink('JavaScript')>> string $code): void {}
final class Encoder {
    <<Hakana\SecurityAnalysis\HtmlSafeJson>>
    public static function encode(mixed $v): string { return json_encode($v, JSON_HEX_TAG) as string; }
}
script('var value = ' . Encoder::encode(input()) . ';');
