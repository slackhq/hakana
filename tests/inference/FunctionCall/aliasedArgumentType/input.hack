namespace A;

function foo(mixed $test): void {
    if ($test is \HH\Awaitable<_>) {
        \HH\Asio\join($test);
    }
}

function bar(mixed $test): void {
    if ($test is Awaitable<_>) {
        Asio\join($test);
    }
}
