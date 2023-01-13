function foo(bool $test, (function(...):mixed) $bar): string {
    try {
        $bar();

        if ($test) {
            return "moo";
        }
        return "miau";
    } catch (\Exception $exception) {
        if ($test) {
            return "moo";
        }
        return "miau";
    }
}