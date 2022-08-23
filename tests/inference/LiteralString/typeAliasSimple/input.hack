<<Hakana\SpecialTypes\LiteralString()>>
type LiteralString = string;

function takesLiteralString(LiteralString $s): void {
}

function foo(string $s) {
    takesLiteralString("foo");
    takesLiteralString("bar" . "baz");
    $a = "bat";
    takesLiteralString("hello $a");
    takesLiteralString($s);
}