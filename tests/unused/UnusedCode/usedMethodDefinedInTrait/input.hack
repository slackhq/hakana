interface I {
    public function foo(): void;
}

trait T2 {
    public function foo(): void {}
}

trait T1 {
    use T2;
}

abstract class Base implements I {}

final class Concrete extends Base {
    use T1;
}

function takes_base(Base $b): void {
    $b->foo();
}

<<__EntryPoint>>
function main(): void {
    takes_base(new Concrete());
}