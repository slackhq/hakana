final class A {
    public static function foo(): void {
        self::bar();
        self::baz();
    }

    protected static function bar(): void {}

    protected static function baz(): void {
        echo 1;
        echo 3;
        echo 2;
    }
}