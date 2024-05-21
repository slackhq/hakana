final class C {
    public static function foo() : array {
        $a = vec[];
        $a[] = (string) $_GET["bad"];
        return $a;
    }

    public static function bar() {
        exec(self::foo()[0]);
    }
}