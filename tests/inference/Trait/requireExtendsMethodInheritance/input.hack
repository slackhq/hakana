trait T1 {
    public function bar(string $s): void {}
}

<<__Sealed(B::class)>>
class A {
    use T1;

    public static function getDefault(string $s) {
        var_dump($s);
    }
}

trait T2 {
    require extends A;

    public function foo(): void {}
}

final class B extends A {
    use T2;

    <<__Override>>
    public static function getDefault(?string $s) {
        var_dump($s);
    }

    <<__Override>>
    public function bar(?string $s): void {
        static::getDefault(null);
    }
}

function foo(): void {
    B::getDefault(null);

    $b = new B();
    $b->foo();
    $b->bar(null);
}
