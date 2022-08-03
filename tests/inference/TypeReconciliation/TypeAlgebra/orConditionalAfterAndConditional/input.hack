function foo(string $a, string $b): void {
    if ($a && $b) {
        echo "a";
    } else if ($a || $b) {
        echo "b";
    }
}