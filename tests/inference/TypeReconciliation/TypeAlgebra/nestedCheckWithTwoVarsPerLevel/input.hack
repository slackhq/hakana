function foo(?stdClass $a, ?stdClass $b, ?stdClass $c, ?stdClass $d): void {
    if ($a !== null && $b !== null) {
        if ($c !== null && $d !== null) {}
    }
}