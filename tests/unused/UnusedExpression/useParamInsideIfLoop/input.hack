function foo() : void {
    $a = 1;

    if (rand(0, 1) !== 0) {
        while (rand(0, 1)) {
            $a = 2;
        }
    }

    echo $a;
}
