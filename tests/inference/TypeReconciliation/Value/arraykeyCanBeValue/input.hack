function foo(arraykey $i): void {
    if ($i !== 'a') {}
    if ($i !== 5) {}
}

function bar(string $i): void {
    if ($i !== 'a') {}
}

function baz(int $i): void {
    if ($i !== 5) {}
}

function bat(?num $start_date, ?num $end_date): void {
    if (($start_date is nonnull && $start_date !== 0) && ($end_date is nonnull && $end_date !== 0)) {}
}