enum A: string {
    B = 'b';
    C = 'c';
    D = 'd';
    E = 'e';
}

function foo(string $s): void {
    /* HAKANA_FIXME[ImpossibleTypeComparison] */
    $a = $s is ?A ? $s : exit();
}