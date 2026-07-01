function foo(): string {
    $d = '';
    if (rand(0, 1) !== 0) {
        $d .= 'a ';
    }
    if (rand(0, 1) !== 0) {
        $d .= 'b ';
    }
    if (rand(0, 1) !== 0) {
        $d .= 'c ';
    }
    if (rand(0, 1) !== 0) {
        $d .= 'd ';
    }
    if (rand(0, 1) !== 0) {
        $d .= 'e ';
    }
    if (rand(0, 1) !== 0) {
        $d .= 'f ';
    }
    if (rand(0, 1) !== 0) {
        $d .= 'g ';
    }
    if (rand(0, 1) !== 0) {
        $d .= 'h ';
    }
    if (rand(0, 1) !== 0) {
        $d .= 'i ';
    }

    if ($d == '') {
        $d = 'foo';
    }

    return $d;
}
