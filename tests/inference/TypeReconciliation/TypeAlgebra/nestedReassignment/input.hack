function foo(?string $a): void {
    if ($a === null) {
        $a = "blah-blah";
    } else {
        $a = rand(0, 1) !== 0 ? "blah" : null;

        if ($a === null) {

        }
    }
}
