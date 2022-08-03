function foo(?string $s): string {
    $s ??= "Hello";
    return $s;
}

function bar(?string $s): string {
    $s = $s ?? "Hello";
    return $s;
}