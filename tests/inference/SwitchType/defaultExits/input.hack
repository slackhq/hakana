function foo(): void {
    $a = rand(0, 10);

    switch ($a) {
        case 1:
        case 2:
            $b = 5;

        default:
            exit(1);
    }

    echo $b;
}
