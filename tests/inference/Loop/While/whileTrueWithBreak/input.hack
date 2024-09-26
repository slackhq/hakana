function foo(): void {
    while (true) {
        $a = 5;
        if (rand(0, 1)) {
            break;
        }
    }

    echo $a;
}