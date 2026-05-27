function takesAnInt(): void {
    $i = 0;

    while (rand(0, 1)) {
        $i++;
        if ($i > 10) {
            break;
        } else {}
    }
}
