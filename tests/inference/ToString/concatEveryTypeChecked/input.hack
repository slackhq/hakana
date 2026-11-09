final class Unprintable {}

function nullable_operand(?string $a): string {
    return 'hello '.$a;
}

function class_behind_string(bool $b, string $s, Unprintable $u): string {
    $a = $b ? $s : $u;
    return 'hello '.$a;
}

function both(?Unprintable $u): string {
    return "hello {$u}";
}

function fine(bool $b, string $s, int $i): string {
    $a = $b ? $s : $i;
    return 'hello '.$a;
}
