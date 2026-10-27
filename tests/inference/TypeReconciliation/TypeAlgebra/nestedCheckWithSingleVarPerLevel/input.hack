function foo(?stdClass $a, ?stdClass $b): void {
    if ($a !== null) {
        if ($b !== null) {}
    }
}