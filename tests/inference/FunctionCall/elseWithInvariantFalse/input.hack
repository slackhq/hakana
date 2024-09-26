function foo(): void {
    if (rand(0, 1)) {
        $a = 5;
    } else {
        invariant(false, 'bad');
    }

    echo $a;
}