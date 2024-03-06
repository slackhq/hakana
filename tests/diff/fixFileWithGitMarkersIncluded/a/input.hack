function foo(): int {
    $a = 1;
    $b = 2;
<<<<<<< HEAD
    $c = 3;
=======
>>>>>>> origin/master
    $d = (): void ==> {
        echo $a;
        echo $b;
    };
    $d();
    return 3;
}