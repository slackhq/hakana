function foo(?stdClass $a, ?stdClass $b, ?stdClass $c, ?stdClass $d): void {
    if ($a && $b) {
        if ($c && $d) {}
    }
}