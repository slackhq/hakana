function takes_int(int $i): void {}

function single_type_on_the_left(int $a, ?int $b): void {
    if ($a === $b) {
        takes_int($b);
    }
}

function single_type_on_the_right(int $a, ?int $b): void {
    if ($b === $a) {
        takes_int($b);
    }
}

function not_narrowed_when_different(int $a, ?int $b): void {
    if ($a !== $b) {
        takes_int($b);
    }
}
