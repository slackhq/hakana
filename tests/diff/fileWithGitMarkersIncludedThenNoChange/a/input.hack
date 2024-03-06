function foo(): int {
    $a = 1;
    $b = 2;
    $d = (): void ==> {
        echo $a;
        echo $b;
    };
    $d();
    return 3;
}