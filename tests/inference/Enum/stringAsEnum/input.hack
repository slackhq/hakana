enum A: string {
    B = 'b';
    C = 'c';
    D = 'd';
    E = 'e';
}

function foo(string $s): void {
    /* HAKANA_FIXME[ImpossibleNullTypeComparison] */
    if ($s is ?A) {
        $a = $s;
    } else {
        exit();
    }
}