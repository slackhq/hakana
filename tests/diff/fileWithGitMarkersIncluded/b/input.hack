function foo(): int {
    $a = 1;
    $b = 2;
    $d = (): void ==> {
        echo $a;
        echo $b;
<<<<<<< HEAD
    };
=======
        echo $a+$b;
    };
>>>>>>> origin/master
    $d();
    return 3;
}