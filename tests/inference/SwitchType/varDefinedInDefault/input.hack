function foo(): void {
    $a = rand(0, 10);

    switch ($a) {
        case 1:
        case 2:
            return;

        default:
            $b = 5;
    }

    echo $b;
}
