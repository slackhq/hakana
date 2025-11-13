final class A {
    const STRING = "x";

    public static function bar(string $s) : bool {
        return !defined("FOO") && HH\Lib\Str\search($s, self::STRING) === 0;
    }
}