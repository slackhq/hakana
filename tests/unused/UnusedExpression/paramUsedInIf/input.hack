function foo(string $a): void {
    if (rand(0, 1) !== 0) {
        echo $a;
    }
}
