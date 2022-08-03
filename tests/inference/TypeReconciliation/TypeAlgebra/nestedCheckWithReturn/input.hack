function foo(?stdClass $a, ?stdClass $b): void {
    if ($a === null) {
        return;
    }

    if ($b) {
        echo "hello";
    }
}