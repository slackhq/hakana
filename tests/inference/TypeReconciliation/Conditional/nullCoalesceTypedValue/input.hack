function foo(?string $s) : string {
    return $s ?? "bar";
}