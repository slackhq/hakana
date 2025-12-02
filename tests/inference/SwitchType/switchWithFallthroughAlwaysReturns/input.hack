function foo(string $s, arraykey $k): int {
    if ($k is string) {
        switch ($s) {
            case 'a':
            case 'b':
                return 0;
            default:
                return 1;
        }
    }
    return $k;
}