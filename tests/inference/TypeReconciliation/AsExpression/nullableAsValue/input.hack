function foo(arraykey $s): void {
    $d = $s ?as int;
    if ($d is null) {}
    if ($d is int) {}
}