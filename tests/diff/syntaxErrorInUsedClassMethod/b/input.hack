final class A {
    public static function foo(): void {}

    public static function toBeChanged(): void {
        $b = new B();
        if (rand(0, 1)) {
            echo $b->bar()
        }
        $c = 5;
        echo $c;
    }
}

final class B {
    public function bar(): int {
        return 5;
    }
}