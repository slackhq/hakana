function foo(): void {
    while (true) {
        $a = 5;
        if (rand(0, 1) !== 0) {
            break;
        }
    }

    echo $a;
}
