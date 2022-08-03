function foo(bool $test, callable $bar): string {
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