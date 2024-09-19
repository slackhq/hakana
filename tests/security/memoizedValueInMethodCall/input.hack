final class A {
    private static string $prev = "";

    public static function getPrevious(string $current): string {
        $prev = self::$prev;
        self::$prev = $current;
        return $prev;
    }
}

A::getPrevious(HH\global_get('_GET')["a"]);
echo A::getPrevious("foo");