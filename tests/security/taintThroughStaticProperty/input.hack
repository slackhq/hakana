final class A {
    private static string $foo = "";

    public static function setFoo(string $s): void {
        self::$foo = $s;
    }

    public static function getFoo(): string {
        return self::$foo;
    }
}

echo A::getFoo();
A::setFoo(HH\global_get('_GET')["a"]);