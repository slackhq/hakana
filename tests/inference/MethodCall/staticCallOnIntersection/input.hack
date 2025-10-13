abstract class A {}

interface I {
    public static function foo(): void;
}

final class AChild extends A implements I {
    <<__Override>>
    public static function foo(): void {}
}

function takes_a(A $a): void {
    if ($a is I) {
        $a::foo();
    }
}

<<__EntryPoint>>
function main(): void {
    takes_a(new AChild());
}