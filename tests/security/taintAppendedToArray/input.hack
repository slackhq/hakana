final class C {
    public static function foo() : array {
        $a = vec[];
        $a[] = (string) HH\global_get('_GET')["bad"];
        return $a;
    }

    public static function bar() {
        exec(self::foo()[0]);
    }
}