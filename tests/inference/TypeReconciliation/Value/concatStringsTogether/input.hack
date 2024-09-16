function foo(): string {
    $d = '';
    if (rand(0, 1)) {
        $d .= 'a ';
    }
    if (rand(0, 1)) {
        $d .= 'b ';
    }
    if (rand(0, 1)) {
        $d .= 'c ';
    }
    if (rand(0, 1)) {
        $d .= 'd ';
    }
    if (rand(0, 1)) {
        $d .= 'e ';
    }
    if (rand(0, 1)) {
        $d .= 'f ';
    }
    if (rand(0, 1)) {
        $d .= 'g ';
    }
    if (rand(0, 1)) {
        $d .= 'h ';
    }
    if (rand(0, 1)) {
        $d .= 'i ';
    }

    if ($d == '') {
        $d = 'foo';
    }

    return $d;
}