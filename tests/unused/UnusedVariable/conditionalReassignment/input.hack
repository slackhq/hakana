function foo() : void {
    $a = 0;

    if (rand(0, 1)) {
        $a = 5;
    }

    echo $a;
}