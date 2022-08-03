function foo(string $a, string $b): string {
    if ($a) {
        // do nothing here
    } else if ($b) {
        $a = null;
    } else {
        return "bad";
    }

    if (!$a) return $b;
    return $a;
}