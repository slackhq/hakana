use type Facebook\XHP\HTML\div;
final class UnsafeValue extends Facebook\XHP\UnsafeAttributeValue_DEPRECATED {
    public function toHTMLString(): string { return '" onmouseover="alert(1)'; }
}
<<Hakana\SecurityAnalysis\Source('RawUserData')>>
function raw_value(): UnsafeValue { return new UnsafeValue(); }
$element = <div data-raw={raw_value()} />;
