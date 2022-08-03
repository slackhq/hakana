function foo(): void {
    $f = false;

    do {
        if (rand(0,1)) {
            $f = true;
            break;
        }
    } while (rand(0,1));

    if ($f) {}
}