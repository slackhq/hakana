<<Hakana\SpecialTypes\LiteralString()>>
type LiteralString = string;

enum MyEnum: LiteralString {
    A = 'a';
    B = 'b';
}

function foo(string $s) {
    $s as MyEnum;
}
