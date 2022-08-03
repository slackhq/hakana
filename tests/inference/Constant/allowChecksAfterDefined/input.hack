class A {
    const STRING = "x";

    public static function bar(string $s) : bool {
        return !defined("FOO") && strpos($s, self::STRING) === 0;
    }
}