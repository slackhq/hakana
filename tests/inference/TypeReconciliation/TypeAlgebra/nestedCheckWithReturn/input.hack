function foo(?stdClass $a, ?stdClass $b): void {
    if ($a === null) {
        return;
    }

    if ($b !== null) {
        echo "hello";
    }
}