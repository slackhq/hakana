final class C {
    private static ?dict<string, string> $map = dict[];

    public static function foo(string $id) : ?string {
        if (isset(self::$map[$id])) {
            return self::$map[$id];
        }

        return null;
    }
}